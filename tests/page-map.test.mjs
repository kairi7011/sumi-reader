import test from 'node:test';
import assert from 'node:assert/strict';
import { PageMap, pageWindow } from '../src/page-map.js';

test('large books keep only a small window of thumbnail buttons', () => {
  for (const total of [400, 10000]) {
    const start = pageWindow(total, 690, 550, 0);
    const end = pageWindow(total, 690, 550, start.height - 550);
    assert.equal(start.first, 1);
    assert.equal(end.last, total);
    assert.ok(start.last - start.first + 1 <= 30);
    assert.ok(end.last - end.first + 1 <= 30);
  }
});

test('a typed destination remains inside the rendered window after resize', () => {
  for (const width of [350, 520, 690]) {
    const { columns } = pageWindow(400, width, 480, 0);
    for (const page of [1, 138, 399, 400]) {
      const top = Math.floor((page - 1) / columns) * 196;
      const window = pageWindow(400, width, 480, Math.max(0, top - 142));
      assert.ok(window.first <= page && window.last >= page);
      assert.ok(window.last <= 400);
    }
  }
});

test('a short document has no phantom thumbnail pages', () => {
  assert.deepEqual(pageWindow(3, 690, 550, 0), { columns: 5, height: 196, first: 1, last: 3 });
});

test('closing and reopening the map keeps two active renders without cancelling shared streams', () => {
  const map = Object.create(PageMap.prototype);
  let cancelled = 0, started = 0;
  map.document = {};
  map.dialog = { open: true, close() { this.open = false; } };
  map.preview = { width: 260, height: 360 };
  map.generation = 0;
  map.getPage = () => 114;
  map.deferredCleanup = new Map();
  map.jobs = new Map([[114, { task: { cancel() { cancelled++; } } }], [115, { task: { cancel() { cancelled++; } } }]]);
  map.cache = new Map(); map.failed = new Set(); map.visible = new Set([200, 201]); map.target = 200;
  map.renderThumbnail = page => { started++; map.jobs.set(page, {}); };
  map.close();
  assert.equal(cancelled, 0);
  assert.equal(map.jobs.size, 2);
  map.dialog.open = true; map.pump();
  assert.equal(started, 0);
  map.jobs.delete(114); map.pump();
  assert.equal(started, 1);
  assert.equal(map.jobs.size, 2);
  assert.ok(map.jobs.has(200));
});

test('page-map handoff protects both pages before navigation has acquired their proxies', async () => {
  const map = Object.create(PageMap.prototype);
  const pdf = { numPages: 400 };
  let current = 114, cleanupCalls = 0, finishNavigation;
  map.document = pdf; map.getDocument = () => pdf; map.getPage = () => current;
  map.dialog = { open: true, close() { this.open = false; } };
  map.preview = { width: 260, height: 360 }; map.generation = 0;
  map.jobs = new Map(); map.target = 200;
  map.deferredCleanup = new Map([200, 201, 202].map(page => [page, { cleanup() { cleanupCalls++; return true; } }]));
  map.onNavigate = page => { assert.equal(page, 200); map.flushCleanup(); return new Promise(resolve => { finishNavigation = resolve; }); };
  map.commit();
  assert.equal(cleanupCalls, 1);
  assert.deepEqual([...map.deferredCleanup.keys()], [200, 201]);
  await new Promise(setImmediate);
  assert.equal(cleanupCalls, 1);
  current = 200; finishNavigation(); await new Promise(setImmediate);
  assert.equal(cleanupCalls, 1);
  current = 114; map.flushCleanup();
  assert.equal(cleanupCalls, 3);
  assert.equal(map.deferredCleanup.size, 0);
});

test('reader pending pages remain protected even without a page-map handoff', () => {
  const map = Object.create(PageMap.prototype);
  map.getPage = () => 114;
  map.getProtectedPages = () => [114, 115, 200, 201];
  let cleaned = [];
  map.deferredCleanup = new Map([114, 115, 200, 201, 202].map(page => [page, { cleanup() { cleaned.push(page); return true; } }]));
  map.flushCleanup();
  assert.deepEqual(cleaned, [202]);
  assert.equal(map.deferredCleanup.size, 4);
});
