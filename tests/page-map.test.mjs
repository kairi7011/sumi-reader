import test from 'node:test';
import assert from 'node:assert/strict';
import { pageWindow } from '../src/page-map.js';

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
