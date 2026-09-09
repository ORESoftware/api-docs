import { constants } from 'node:fs';
import { lstat, open, realpath } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep, win32 } from 'node:path';

const MAX_BYTES = 8 * 1024 * 1024;

function requireCondition(condition, message) {
  if (!condition) throw new Error(message);
}

export function validRelativePath(value) {
  return typeof value === 'string' && value.length > 0 && value.length <= 512
    && !/[\\\u0000-\u001f\u007f]/u.test(value)
    && !isAbsolute(value) && !win32.isAbsolute(value) && !/^[A-Za-z]:/u.test(value)
    && !value.split('/').some((part) => part === '' || part === '.' || part === '..');
}

// Canonicalize the caller-owned root once. Below it, even an in-root symlink is
// forbidden: a lexical containment check alone cannot bind evidence to a file.
export async function evidencePath(rootPath, relativePath) {
  requireCondition(validRelativePath(relativePath), 'evidence path must be a normalized relative POSIX path');
  const suppliedRoot = resolve(rootPath);
  const rootStat = await lstat(suppliedRoot);
  requireCondition(rootStat.isDirectory() && !rootStat.isSymbolicLink(), 'evidence root must be a real directory');
  const root = await realpath(suppliedRoot);
  let cursor = root;
  const parts = relativePath.split('/');
  for (const part of parts.slice(0, -1)) {
    cursor = join(cursor, part);
    const stat = await lstat(cursor);
    requireCondition(stat.isDirectory() && !stat.isSymbolicLink(), 'evidence ancestors must be real directories');
  }
  const path = join(root, ...parts);
  const rendered = relative(root, path);
  requireCondition(rendered !== '' && rendered !== '..' && !rendered.startsWith(`..${sep}`) && !isAbsolute(rendered), 'evidence path escapes the configured root');
  return { root, path };
}

function regular(stat, maxBytes) {
  requireCondition(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1n, 'evidence path must be a singly linked regular file');
  requireCondition(stat.size <= BigInt(maxBytes), 'evidence file exceeds the configured byte limit');
}

function sameFile(left, right) {
  return ['dev', 'ino', 'mode', 'nlink', 'size', 'mtimeNs', 'ctimeNs']
    .every((key) => left[key] === right[key]);
}

export async function readSafeBytes(rootPath, relativePath, maxBytes = MAX_BYTES) {
  requireCondition(Number.isSafeInteger(maxBytes) && maxBytes > 0 && maxBytes <= MAX_BYTES, 'evidence byte limit is invalid');
  const { root, path } = await evidencePath(rootPath, relativePath);
  const before = await lstat(path, { bigint: true });
  regular(before, maxBytes);
  // O_NONBLOCK prevents a replaced FIFO from hanging the admission process.
  const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const opened = await file.stat({ bigint: true });
    regular(opened, maxBytes);
    requireCondition(sameFile(before, opened), 'evidence file changed before it was opened');
    const size = Number(opened.size);
    const bytes = Buffer.alloc(size + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await file.read(bytes, offset, bytes.length - offset, offset);
      if (bytesRead === 0) break;
      offset += bytesRead;
    }
    const after = await file.stat({ bigint: true });
    const checked = await evidencePath(root, relativePath);
    const current = await lstat(checked.path, { bigint: true });
    requireCondition(offset === size && sameFile(opened, after) && sameFile(after, current), 'evidence file changed while it was read');
    return bytes.subarray(0, offset);
  } finally {
    await file.close();
  }
}

export async function readSafeJson(root, relativePath) {
  const bytes = await readSafeBytes(root, relativePath);
  try {
    // Buffer.toString replaces malformed UTF-8 with U+FFFD, which can turn
    // corrupt evidence into an apparently valid, but different, JSON value.
    const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
    return JSON.parse(text);
  } catch {
    throw new Error('evidence file must contain valid UTF-8 JSON');
  }
}

// Create only missing, real directories under the caller-owned root. No cleanup
// path ever removes a pre-existing temporary file or an unowned destination.
export async function ensureEvidenceParents(rootPath, relativePath) {
  requireCondition(validRelativePath(relativePath), 'output path must be a normalized relative POSIX path');
  const { mkdir } = await import('node:fs/promises');
  const root = resolve(rootPath);
  const stat = await lstat(root);
  requireCondition(stat.isDirectory() && !stat.isSymbolicLink(), 'evidence root must be a real directory');
  let cursor = await realpath(root);
  for (const part of relativePath.split('/').slice(0, -1)) {
    cursor = join(cursor, part);
    try { await mkdir(cursor); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    const parent = await lstat(cursor);
    requireCondition(parent.isDirectory() && !parent.isSymbolicLink(), 'evidence ancestors must be real directories');
  }
  return evidencePath(root, relativePath);
}

export async function writeOwnedJson(rootPath, relativePath, text, ownedSchemas) {
  const { randomUUID } = await import('node:crypto');
  const { rename, unlink } = await import('node:fs/promises');
  const { dirname } = await import('node:path');
  requireCondition(typeof text === 'string' && Buffer.byteLength(text) <= MAX_BYTES, 'output exceeds the configured byte limit');
  const { root, path } = await ensureEvidenceParents(rootPath, relativePath);
  let previous = null;
  try { previous = await lstat(path, { bigint: true }); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (previous !== null) {
    const current = await readSafeJson(root, relativePath);
    requireCondition(ownedSchemas.has(current?.schema), 'refusing to replace an output not owned by this admission tool');
  }
  const temporary = join(dirname(path), `.projection-evidence-${randomUUID()}.tmp`);
  const file = await open(temporary, 'wx', 0o600);
  try {
    await file.writeFile(text, 'utf8');
    await file.sync();
    await evidencePath(root, relativePath);
    let current = null;
    try { current = await lstat(path, { bigint: true }); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    requireCondition(previous === null ? current === null : current !== null && sameFile(previous, current), 'output changed while evidence was prepared');
    await rename(temporary, path);
  } finally {
    await file.close();
    try { await unlink(temporary); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
}
