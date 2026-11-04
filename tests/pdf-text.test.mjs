import test from 'node:test';
import assert from 'node:assert/strict';
import { readPageText } from '../src/pdf-text.js';

test('PDF text works with WebKit streams that have no async iterator', async () => {
  let released = false;
  const chunks = [
    { value: { items: [{ str: '日本語' }], styles: { font1: { vertical: false } }, lang: 'ja' }, done: false },
    { value: { items: [{ str: 'の本' }], styles: {}, lang: null }, done: false },
    { done: true },
  ];
  const content = await readPageText({ streamTextContent: () => ({ getReader: () => ({
    read: async () => chunks.shift(), releaseLock: () => { released = true; },
  }) }) });
  assert.equal(content.items.map(item => item.str).join(''), '日本語の本');
  assert.equal(content.lang, 'ja');
  assert.equal(content.styles.font1.vertical, false);
  assert.equal(released, true);
});

test('a failed extraction releases its stream reader', async () => {
  let released = false;
  await assert.rejects(readPageText({ streamTextContent: () => ({ getReader: () => ({
    read: async () => { throw new Error('Document closed'); }, releaseLock: () => { released = true; },
  }) }) }), /Document closed/);
  assert.equal(released, true);
});
