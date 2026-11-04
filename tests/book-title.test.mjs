import test from 'node:test';
import assert from 'node:assert/strict';
import { cleanBookTitle, titleFromMetadata, titleFromCover, deriveBookTitle } from '../src/book-title.js';

const text = (str, height, x, y, width = str.length * height * .5) => ({ str, height, width, transform: [height, 0, 0, height, x, y] });

test('production identifiers do not become displayed book names', () => {
  for (const title of ['oreilly-978-0-0000-0000-1e.pdf', '978-0-0000-0000-1', 'ISBN: 9780000000001', 'Untitled-1', '無題 2', 'Book_export_v3', '20241104_本文_校了', 'BK_0001837', 'Microsoft Word - Document1', 'Front Cover', '0c6c8849-2122-4fdd-a838-35948707eaa3']) {
    assert.equal(cleanBookTitle(title), null, title);
  }
  assert.equal(cleanBookTitle('Data Science at Scale'), 'Data Science at Scale');
  assert.equal(cleanBookTitle('The Book of Proof'), 'The Book of Proof');
  assert.equal(cleanBookTitle('1984'), '1984');
});

test('useful metadata retains Japanese wording and a readable edition', () => {
  assert.equal(cleanBookTitle('\ufeff架空の読書ノート八第 ４ 版\u0000'), '架空の読書ノート八 第4版');
  assert.equal(cleanBookTitle('架空の読書ノート二 第2版'), '架空の読書ノート二 第2版');
  assert.equal(cleanBookTitle('Quiet Reading Notes C'), 'Quiet Reading Notes C');
});

test('XMP supplies a real title when Info contains a generated filename', () => {
  const metadata = { info: { Title: 'book_cover_final.indd' }, metadata: { get: key => key === 'dc:title' ? '読みやすい画面の設計' : null } };
  assert.equal(titleFromMetadata(metadata, '12345678.pdf'), '読みやすい画面の設計');
  assert.equal(titleFromMetadata({ info: { Title: '正式な書名' }, metadata: { get() { throw Error('damaged XMP'); } } }), '正式な書名');
});

test('localized XMP alternatives prefer the default title', () => {
  assert.equal(titleFromMetadata({ metadata: { get: () => ({ 'x-default': 'Designing Interfaces', ja: 'インターフェースの設計' }) } }), 'Designing Interfaces');
});

test('a Japanese cover joins split glyph runs and stacked title lines, retaining an edition', () => {
  const content = { items: [
    text('画面', 34, 50, 700, 68), text('の', 34, 118, 700, 34), text('設計', 34, 152, 700, 68),
    text('読みやすさの原則', 32, 50, 658, 256),
    text('第 ２ 版', 15, 50, 622, 70),
    text('山田 太郎 著', 20, 50, 460), text("O’REILLY", 46, 50, 50),
  ] };
  assert.equal(titleFromCover(content), '画面の設計読みやすさの原則 第2版');
});

test('Latin cover words preserve their spacing and lower-size subtitles stay out', () => {
  const content = { items: [
    text('Designing', 40, 50, 700, 160), text('Calm', 40, 225, 700, 100),
    text('Interfaces', 38, 50, 650, 180),
    text('Second Edition', 16, 50, 612, 120),
    text('A practical guide to a quieter working day', 12, 50, 555),
  ] };
  assert.equal(titleFromCover(content), 'Designing Calm Interfaces Second Edition');
});

test('an author printed larger than the title is excluded using metadata', () => {
  const content = { items: [text('Alex Smith', 70, 50, 760), text('Small Interfaces', 32, 50, 600), text('A field guide', 14, 50, 530)] };
  assert.equal(titleFromCover(content, { author: 'Alex Smith' }), 'Small Interfaces');
});

test('image-only covers and body paragraphs produce no invented title', () => {
  assert.equal(titleFromCover({ items: [] }), null);
  assert.equal(titleFromCover({ items: Array.from({ length: 8 }, (_, i) => text(`This is paragraph line ${i + 1}.`, 12, 40, 740 - i * 16)) }), null);
  assert.equal(titleFromCover({ items: [text('Top Heading', 30, 40, 720), text('Unrelated Heading', 30, 40, 200)] }), null);
});

test('valid metadata avoids fetching or extracting cover pages', async () => {
  const pdf = { async getMetadata() { return { info: { Title: '架空の読書ノート五 第2版' } }; }, getPage() { assert.fail('cover must not be requested'); } };
  assert.equal(await deriveBookTitle(pdf, 'oreilly-978-0-00000-000-8e.pdf'), '架空の読書ノート五 第2版');
});

test('fallback reads only the cover and does not clean up a page shared with the reader', async () => {
  const requested = [];
  const pdf = {
    async getMetadata() { return { info: { Title: 'Untitled' } }; },
    async getPage(page) {
      requested.push(page);
      return {
        streamTextContent() { return new ReadableStream({ start(controller) { controller.enqueue({ items: [text('A Quiet Reader', 40, 40, 700)], styles: {} }); controller.close(); } }); },
        cleanup() { assert.fail('the reader owns shared PDF page resources'); },
      };
    },
  };
  assert.equal(await deriveBookTitle(pdf, 'scan_123456.pdf'), 'A Quiet Reader');
  assert.deepEqual(requested, [1]);
  assert.equal(await deriveBookTitle({ async getMetadata() { throw Error('bad metadata'); }, async getPage() { throw Error('bad cover'); } }), null);
});
