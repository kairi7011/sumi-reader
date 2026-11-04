import { readPageText } from './pdf-text.js';

const CJK = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u;
const EDITION = /^(?:(?:改訂|増補)\s*)?第\s*\d+\s*版$|^(?:\d+(?:st|nd|rd|th)|first|second|third|fourth|fifth)\s+edition$/i;
const GENERIC = /^(?:untitled|無題|名称未設定|新規(?:書類|ドキュメント)|document|pdf|scan|スキャン|cover|front cover|back cover|body|frontmatter|backmatter|表紙|本文|奥付|output|final|proof)(?:[\s_.-]*(?:\d+|v\d+|final|proof|校了|入稿))*$/i;
const PRODUCTION = /(?:^|[\s_.-])(?:cover|body|final|proof|print|export|press|output|表紙|本文|入稿|校了|初校|再校)(?:$|[\s_.-]|\d)/i;

function tidy(value) {
  return typeof value === 'string' ? value.normalize('NFKC').replace(/[\u0000-\u001f\u007f\u200b-\u200f\ufeff]/g, ' ').replace(/\s+/g, ' ').trim() : '';
}

export function cleanBookTitle(value, filename = '') {
  let title = tidy(value);
  if (title.length < 2 || title.length > 220 || (!/[\p{L}]/u.test(title) && !/^\d{2,4}$/.test(title))) return null;
  if (GENERIC.test(title) || /^(?:Adobe\s+InDesign|Microsoft\s+(?:Word|PowerPoint)|Acrobat)(?:\b|\s)/i.test(title)) return null;
  if (/\.(?:pdf|indd|idml|docx?|pptx?|ai|eps|ps)$/i.test(title) || /^(?:[a-z]:[\\/]|[~.]*\/)/i.test(title)) return null;
  if (/^(?:urn:)?(?:uuid:)?[a-f\d]{8}(?:-[a-f\d]{4}){3}-[a-f\d]{12}$/i.test(title) || /^[a-f\d]{24,}$/i.test(title)) return null;
  if (/(?:97[89][\d\s-]{10,}|ISBN[\s: -]*[\dX-]{8,})/i.test(title)) return null;
  if (/^[a-z]{0,10}[-_ ]?\d{5,}(?:[-_][a-z\d]+)*$/i.test(title)) return null;
  if ((/[_]|\d{4,}|\bv\d+(?:\.\d+)*\b/i.test(title) && PRODUCTION.test(title)) || /^\d{6,}[-_]/.test(title)) return null;
  const base = filename.split(/[\\/]/).pop()?.replace(/\.pdf$/i, '') || '';
  if (title.toLocaleLowerCase() === tidy(base).toLocaleLowerCase() && /[_]|\d{6,}/.test(title)) return null;
  title = title.replace(/第\s*(\d+)\s*版/g, '第$1版').replace(/(\S)(第\d+版)/g, '$1 $2');
  return title;
}

export function titleFromMetadata(metadata, filename = '') {
  const values = [metadata?.info?.Title];
  try {
    const xmp = metadata?.metadata?.get?.('dc:title');
    if (typeof xmp === 'string') values.push(xmp);
    else if (Array.isArray(xmp)) values.push(...xmp);
    else if (xmp && typeof xmp === 'object') values.push(xmp['x-default'], ...Object.values(xmp));
  } catch { /* A malformed XMP packet should not hide a usable Info title. */ }
  for (const value of values) {
    const title = cleanBookTitle(value, filename);
    if (title) return title;
  }
  return null;
}

function coverLines(content) {
  const lines = [];
  for (const item of (content?.items || []).slice(0, 1200)) {
    const text = tidy(item.str);
    const transform = item.transform;
    if (!text || !Array.isArray(transform) || transform.length < 6) continue;
    const height = Math.abs(item.height) || Math.hypot(transform[2], transform[3]);
    if (!Number.isFinite(height) || height <= 0 || Math.abs(transform[1]) > Math.abs(transform[0]) * .2) continue;
    const part = { text, height, x: transform[4], y: transform[5], width: Math.abs(item.width) || text.length * height * .5 };
    if (![part.x, part.y, part.width].every(Number.isFinite)) continue;
    let line = lines.find(line => Math.abs(line.y - part.y) < Math.max(line.height, height) * .22);
    if (!line) { line = { y: part.y, height, parts: [] }; lines.push(line); }
    line.height = Math.max(line.height, height);
    line.parts.push(part);
  }
  return lines.map(line => {
    line.parts.sort((a, b) => a.x - b.x);
    let text = '';
    let previous;
    for (const part of line.parts) {
      const gap = previous ? part.x - previous.x - previous.width : 0;
      const separate = previous && !CJK.test(previous.text.at(-1) + part.text[0]) && gap > Math.min(previous.height, part.height) * .08;
      text += (separate ? ' ' : '') + part.text;
      previous = part;
    }
    return { text: tidy(text), height: line.height, y: line.y, x: line.parts[0].x, right: Math.max(...line.parts.map(part => part.x + part.width)) };
  }).sort((a, b) => b.y - a.y);
}

function joins(a, b) {
  const overlap = Math.min(a.right, b.right) - Math.max(a.x, b.x);
  const aligned = Math.abs(a.x - b.x) < Math.max(a.height, b.height) || Math.abs(a.x + a.right - b.x - b.right) < Math.max(a.height, b.height) * 2;
  return Math.abs(a.y - b.y) <= Math.max(a.height, b.height) * 2 && (overlap > 0 || aligned);
}

export function titleFromCover(content, { filename = '', author = '' } = {}) {
  const authorNames = tidy(author).split(/[,;、]|\s+(?:and|&)\s+/i).map(tidy).filter(Boolean);
  const lines = coverLines(content).filter(line => {
    if (authorNames.includes(line.text) || /(?:著|編著|監修|監訳|訳)$|^by\s|copyright|https?:\/\/|©/i.test(line.text)) return false;
    if (/^O['’]?REILLY(?:\s+MEDIA)?$|^オライリー(?:・ジャパン)?$|\b(?:PRESS|PUBLISHING|PUBLISHERS)\s*$/i.test(line.text)) return false;
    return EDITION.test(line.text) || Boolean(cleanBookTitle(line.text, filename));
  });
  const titleLines = lines.filter(line => !EDITION.test(line.text));
  if (!titleLines.length) return null;
  const largest = Math.max(...titleLines.map(line => line.height));
  const prominent = titleLines.filter(line => line.height >= largest * .75);
  // Uniform body text offers no reliable evidence that its first sentence is a title.
  if (prominent.length > 4 || (titleLines.length > 3 && titleLines.every(line => line.height >= largest * .75))) return null;
  const seed = prominent.reduce((best, line) => line.height > best.height ? line : best);
  const selected = new Set([seed]);
  for (let pass = 0; pass < prominent.length; pass++) {
    for (const line of prominent) if ([...selected].some(neighbor => joins(neighbor, line))) selected.add(line);
  }
  if (selected.size !== prominent.length) return null;
  const ordered = [...selected].sort((a, b) => b.y - a.y);
  let title = ordered.map(line => line.text).reduce((a, b) => a ? a + (CJK.test(a.at(-1) + b[0]) ? '' : ' ') + b : b, '');
  const edition = lines.find(line => EDITION.test(line.text) && ordered.some(other => joins(other, line)));
  if (edition) title += ` ${edition.text}`;
  return cleanBookTitle(title, filename);
}

export async function deriveBookTitle(pdf, filename = '') {
  let metadata;
  try {
    metadata = await pdf.getMetadata();
    const title = titleFromMetadata(metadata, filename);
    if (title) return title;
  } catch { /* Cover text can still be available when metadata is damaged. */ }
  try {
    const page = await pdf.getPage(1);
    return titleFromCover(await readPageText(page), { filename, author: metadata?.info?.Author });
  } catch { return null; }
}
