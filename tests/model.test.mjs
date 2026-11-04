import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeState, visiblePages, cropRect, pageText, citation, searchMatches } from '../src/model.js';

test('restored state cannot navigate outside a replaced or shortened document', () => {
  const state = normalizeState({ page: 99, zoom: 100, bookmarks: [3, 3, 0, 99, 1], mode: 'unknown' }, 4);
  assert.equal(state.page, 4);
  assert.equal(state.zoom, 3);
  assert.deepEqual(state.bookmarks, [1, 3]);
  assert.equal(state.mode, 'single');
});

test('a spread at the end does not request a missing page', () => {
  assert.deepEqual(visiblePages(4, 5, 'spread'), [4, 5]);
  assert.deepEqual(visiblePages(5, 5, 'spread'), [5]);
});

test('a backwards capture drag is bounded by the PDF page', () => {
  assert.deepEqual(cropRect({ x: 110, y: 80 }, { x: -10, y: 20 }, 100, 90), { x: 0, y: 20, width: 100, height: 60 });
});

test('text extraction preserves line breaks and omits non-text PDF markers', () => {
  assert.equal(pageText({ items: [{ str: 'Hello', hasEOL: false }, { type: 'beginMarkedContent' }, { str: 'world', hasEOL: true }, { str: 'Next', hasEOL: true }] }), 'Hello world\nNext');
  assert.equal(citation('Book.pdf', 7, '  A quote  '), 'Book.pdf — p. 7\n\nA quote');
});

test('search accepts Japanese full-width Latin text and does not match an empty query', () => {
  assert.equal(searchMatches('ＡＩとAI', 'ai').length, 2);
  assert.deepEqual(searchMatches('test', '  '), []);
});
