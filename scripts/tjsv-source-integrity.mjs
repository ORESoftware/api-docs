import { execFile as execFileCallback } from 'node:child_process';
import { createHash } from 'node:crypto';
import { realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
import { promisify } from 'node:util';
import { readSafeBytes } from './projection-evidence-io.mjs';

const execFile = promisify(execFileCallback);

// Check bytes, not just HEAD or git status. assume-unchanged/skip-worktree can
// hide modified files from an index-based status check. Dependencies remain a
// separate npm-ci/lockfile boundary; this verifies the pinned repository source.
export async function verifyValidatorSource(rootPath, expectedRevision) {
  if (!/^[a-f0-9]{40}$/u.test(expectedRevision)) throw new Error('validator revision is invalid');
  const root = resolve(rootPath);
  const env = { ...process.env, GIT_NO_REPLACE_OBJECTS: '1' };
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES']) delete env[key];
  const git = async (...args) => (await execFile('git', ['-C', root, ...args], {
    encoding: 'utf8', maxBuffer: 8 * 1024 * 1024, timeout: 30000, env,
  })).stdout;
  const head = (await git('rev-parse', 'HEAD')).trim();
  if (head !== expectedRevision) throw new Error('validator checkout does not match the pinned revision');
  if (await realpath((await git('rev-parse', '--show-toplevel')).trim()) !== await realpath(root)) {
    throw new Error('validator root must be the repository root');
  }
  const tree = await git('ls-tree', '-rz', '--full-tree', 'HEAD');
  const entries = tree.split('\0').filter(Boolean);
  if (entries.length === 0) throw new Error('validator source tree is empty');
  for (const entry of entries) {
    const match = /^(100644|100755) blob ([a-f0-9]{40})\t(.+)$/u.exec(entry);
    if (!match) throw new Error('validator source must contain only regular tracked files');
    const bytes = await readSafeBytes(root, match[3]);
    const actual = createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    if (actual !== match[2]) throw new Error('validator tracked source differs from the pinned revision');
  }
  // Include ignored files here: an extra imported source cannot be blessed by
  // hiding it in .gitignore. Installed dependencies outside src/bin are separate.
  if ((await git('ls-files', '--others', '-z', '--', 'src', 'bin')).length !== 0) {
    throw new Error('validator source contains untracked files');
  }
}
