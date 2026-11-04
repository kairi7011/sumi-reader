const path = require('node:path');

const MAX_PDF_BYTES = 250 * 1024 * 1024;
const MAX_IMAGE_BYTES = 32 * 1024 * 1024;
const APP_ORIGIN = 'app://sumi';
const APP_URL = `${APP_ORIGIN}/src/index.html`;
const ASSET_PREFIXES = ['src/', 'assets/', 'node_modules/pdfjs-dist/'];

function resolveAsset(root, rawURL) {
  const url = new URL(rawURL);
  if (url.protocol !== 'app:' || url.hostname !== 'sumi' || url.port || url.username || url.password) {
    throw new Error('Unsupported origin');
  }
  const relative = decodeURIComponent(url.pathname).replace(/^\/+/, '');
  if (relative.includes('\0') || relative.includes('\\') || !ASSET_PREFIXES.some(prefix => relative.startsWith(prefix))) {
    throw new Error('Unsupported asset');
  }
  const resolved = path.resolve(root, relative);
  if (!resolved.startsWith(`${path.resolve(root)}${path.sep}`)) throw new Error('Invalid asset path');
  const normalized = path.relative(root, resolved).split(path.sep).join('/');
  if (!ASSET_PREFIXES.some(prefix => normalized.startsWith(prefix))) throw new Error('Invalid asset path');
  return resolved;
}

function sanitizeState(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid reading state');
  const encoded = JSON.stringify(value);
  if (Buffer.byteLength(encoded) > 64 * 1024) throw new Error('Reading state is too large');
  const result = {};
  for (const key of ['page', 'zoom', 'fit', 'mode', 'direction', 'theme', 'bookmarks']) {
    if (Object.hasOwn(value, key)) result[key] = value[key];
  }
  if ('page' in result && (!Number.isInteger(result.page) || result.page < 1 || result.page > 1_000_000)) throw new Error('Invalid page');
  if ('zoom' in result && (!Number.isFinite(result.zoom) || result.zoom < 0.1 || result.zoom > 10)) throw new Error('Invalid zoom');
  for (const key of ['fit', 'mode', 'direction', 'theme']) {
    if (key in result && (typeof result[key] !== 'string' || result[key].length > 32)) throw new Error(`Invalid ${key}`);
  }
  if ('bookmarks' in result && (!Array.isArray(result.bookmarks) || result.bookmarks.length > 2000 || !result.bookmarks.every(page => Number.isInteger(page) && page > 0 && page <= 1_000_000))) throw new Error('Invalid bookmarks');
  return result;
}

function parsePngDataURL(value) {
  if (typeof value !== 'string' || value.length > Math.ceil(MAX_IMAGE_BYTES * 4 / 3) + 100 || !/^data:image\/png;base64,[a-zA-Z0-9+/]+={0,2}$/.test(value)) throw new Error('Invalid PNG image');
  const bytes = Buffer.from(value.slice(value.indexOf(',') + 1), 'base64');
  if (bytes.length > MAX_IMAGE_BYTES || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) throw new Error('Invalid PNG image');
  if (bytes.length < 24 || bytes.toString('ascii', 12, 16) !== 'IHDR') throw new Error('Invalid PNG image');
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  if (!width || !height || width > 12000 || height > 12000 || width * height > 32_000_000) throw new Error('PNG image is too large');
  return bytes;
}

module.exports = { APP_ORIGIN, APP_URL, MAX_PDF_BYTES, resolveAsset, sanitizeState, parsePngDataURL };
