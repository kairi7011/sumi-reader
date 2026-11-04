import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { resolveAsset, sanitizeState, parsePngDataURL } = require('../electron/policy.cjs');

test('only local app assets are served, including encoded traversal attempts', () => {
  assert.equal(resolveAsset('/app', 'app://sumi/src/index.html'), '/app/src/index.html');
  assert.equal(resolveAsset('/app', 'app://sumi/node_modules/pdfjs-dist/build/pdf.worker.mjs'), '/app/node_modules/pdfjs-dist/build/pdf.worker.mjs');
  for (const url of ['https://sumi/src/index.html', 'app://other/src/index.html', 'app://sumi/electron/main.cjs', 'app://sumi/src/..%2felectron/main.cjs', 'app://sumi/src/%2e%2e%2f%2e%2e%2fsecret', 'app://sumi/src/%00', 'app://sumi/src/..%5csecret']) assert.throws(() => resolveAsset('/app', url));
});

test('reading preferences reject invalid values and omit unrelated keys', () => {
  assert.deepEqual(sanitizeState({ page: 4, zoom: 1.2, theme: 'paper', bookmarks: [1, 10], path: '/private.pdf' }), { page: 4, zoom: 1.2, theme: 'paper', bookmarks: [1, 10] });
  for (const state of [{ page: 0 }, { page: 1.5 }, { zoom: Infinity }, { bookmarks: ['1'] }, { theme: {} }, { notes: 'a'.repeat(70_000) }]) assert.throws(() => sanitizeState(state));
});

test('clipboard images must be bounded PNG payloads', () => {
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64');
  assert.deepEqual(parsePngDataURL(`data:image/png;base64,${png.toString('base64')}`), png);
  const oversized = Buffer.from(png);
  oversized.writeUInt32BE(12001, 16);
  assert.throws(() => parsePngDataURL(`data:image/png;base64,${oversized.toString('base64')}`));
  for (const value of ['data:image/jpeg;base64,aGVsbG8=', 'data:image/png;base64,aGVsbG8=', 'https://example.com/a.png', null]) assert.throws(() => parsePngDataURL(value));
});
