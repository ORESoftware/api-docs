import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const ROOT = fileURLToPath(new URL('../', import.meta.url));
const read = path => readFile(new URL(path, `file://${ROOT}/`), 'utf8');
const extractJsonScript = (html, id) => {
  const escaped = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = html.match(new RegExp(`<script\\s+type=["']application/json["']\\s+id=["']${escaped}["']>([\\s\\S]*?)<\\/script>`, 'i'));
  assert.ok(match, `missing JSON example ${id}`);
  return JSON.parse(match[1].trim());
};

const callKeys = new Set(['v', 'op', 'id', 'key', 'transport', 'path', 'query', 'headers', 'body', 'traceId', 'spanId']);
const receiptKeys = new Set(['v', 'op', 'id', 'key', 'transport', 'ok', 'status', 'body', 'error', 'traceId', 'spanId']);
const keyPattern = /^[A-Za-z][A-Za-z0-9_]*$/;
const transports = new Set(['http', 'tcp', 'websocket', 'nats']);

function assertCall(value) {
  assert.equal(value.v, 1);
  assert.equal(value.op, 'call');
  assert.ok(typeof value.id === 'string' && value.id.length >= 1 && value.id.length <= 128);
  assert.ok(typeof value.key === 'string' && keyPattern.test(value.key));
  if (value.transport !== undefined) assert.ok(transports.has(value.transport));
  assert.ok(Object.keys(value).every(key => callKeys.has(key)), 'call example contains a schema-unknown field');
}

function assertReceipt(value) {
  assert.equal(value.v, 1);
  assert.equal(value.op, 'receipt');
  assert.ok(typeof value.id === 'string' && value.id.length >= 1 && value.id.length <= 128);
  assert.ok(typeof value.key === 'string' && keyPattern.test(value.key));
  assert.equal(typeof value.ok, 'boolean');
  if (value.transport !== undefined) assert.ok(transports.has(value.transport));
  if (value.status !== undefined) assert.ok(Number.isInteger(value.status) && value.status >= 100 && value.status <= 599);
  assert.ok(Object.keys(value).every(key => receiptKeys.has(key)), 'receipt example contains a schema-unknown field');
  if (value.ok) {
    assert.equal(value.error, undefined);
    if (value.status !== undefined) assert.ok(value.status >= 200 && value.status <= 399);
  } else {
    assert.ok(value.error && typeof value.error === 'object' && !Array.isArray(value.error));
    assert.equal(value.body, undefined);
    if (value.status !== undefined) assert.ok(value.status >= 400 && value.status <= 599);
  }
}

test('Pages demo contains required RPC architecture and use-case surfaces', async () => {
  const html = await read('gh-pages/index.html');
  for (const fragment of [
    'Peer authorities, not a generator hierarchy',
    'Envelope sanity playground',
    'HTTP JSON services',
    'WebSocket clients',
    'TCP services',
    'NATS adapters',
    'Cross-language SDKs',
    'Deployment drift defense',
    'ORESoftware/typespec-json-schema-validator',
  ]) assert.ok(html.includes(fragment), `missing demo fragment: ${fragment}`);
});

test('Pages demo tracks the exact TJSV revision used by RPC admission', async () => {
  const [html, admission] = await Promise.all([
    read('gh-pages/index.html'),
    read('scripts/tjsv-rpc-admission.mjs'),
  ]);
  const match = admission.match(/export const TJSV_REVISION = '([0-9a-f]{40})';/);
  assert.ok(match, 'unable to resolve TJSV_REVISION');
  assert.ok(html.includes(match[1]), 'Pages demo TJSV revision drifted from executable RPC admission');
});

test('embedded call and receipt examples obey the authored v1 envelope constraints', async () => {
  const html = await read('gh-pages/index.html');
  assertCall(extractJsonScript(html, 'rpc-call-example'));
  assertReceipt(extractJsonScript(html, 'rpc-receipt-example'));
  assertReceipt(extractJsonScript(html, 'rpc-error-example'));
});

test('Pages demo remains dependency-free and does not contain credential-shaped literals', async () => {
  const html = await read('gh-pages/index.html');
  assert.doesNotMatch(html, /<script\b[^>]*\bsrc\s*=\s*["']https?:\/\//i);
  assert.doesNotMatch(html, /<link\b[^>]*\bhref\s*=\s*["']https?:\/\//i);
  assert.doesNotMatch(html, /url\(\s*["']?https?:\/\//i);
  assert.doesNotMatch(html, /\bghp_[A-Za-z0-9]{20,}\b/);
  assert.doesNotMatch(html, /\blin_api_[A-Za-z0-9]{20,}\b/);
  assert.ok(html.includes('no CDN dependencies'));
});

test('Pages workflow verifies PRs and deploys only the gh-pages source from main', async () => {
  const workflow = await read('.github/workflows/pages.yml');
  assert.match(workflow, /pull_request:/);
  assert.match(workflow, /branches:\s*\n\s*- main/);
  assert.match(workflow, /node --test scripts\/test-gh-pages-rpc-demo\.mjs/);
  assert.match(workflow, /path:\s*gh-pages/);
  assert.match(workflow, /github\.event_name != 'pull_request'/);
  const uses = [...workflow.matchAll(/uses:\s*[^@\s]+@([^\s#]+)/g)].map(match => match[1]);
  assert.ok(uses.length >= 4, 'expected checkout, setup-node, upload-pages-artifact, and deploy-pages actions');
  assert.ok(uses.every(ref => /^[0-9a-f]{40}$/.test(ref)), 'all GitHub Actions dependencies must be immutable 40-character SHAs');
});
