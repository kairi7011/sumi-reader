export const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

export function normalizeState(value = {}, total = 1) {
  return {
    page: clamp(Number.isFinite(value.page) ? Math.floor(value.page) : 1, 1, total),
    zoom: clamp(Number(value.zoom) || 1, 0.35, 3),
    fit: ['page', 'width', 'custom'].includes(value.fit) ? value.fit : 'page',
    mode: value.mode === 'spread' ? 'spread' : 'single',
    direction: value.direction === 'rtl' ? 'rtl' : 'ltr',
    theme: ['paper', 'white', 'night'].includes(value.theme) ? value.theme : 'paper',
    bookmarks: [...new Set((Array.isArray(value.bookmarks) ? value.bookmarks : []).filter(p => Number.isInteger(p) && p >= 1 && p <= total))].sort((a, b) => a - b),
  };
}

export function visiblePages(page, total, mode) {
  const first = clamp(page, 1, total);
  return mode === 'spread' && first < total ? [first, first + 1] : [first];
}

export function cropRect(start, end, width, height) {
  const x1 = clamp(Math.min(start.x, end.x), 0, width);
  const y1 = clamp(Math.min(start.y, end.y), 0, height);
  const x2 = clamp(Math.max(start.x, end.x), 0, width);
  const y2 = clamp(Math.max(start.y, end.y), 0, height);
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

export function pageText(content) {
  let result = '';
  for (const item of content.items) {
    if (!('str' in item)) continue;
    result += item.str;
    if (item.hasEOL) result += '\n';
    else if (item.str && !/\s$/.test(item.str)) result += ' ';
  }
  return result.replace(/[ \t]+\n/g, '\n').trim();
}

export function citation(name, page, text) {
  return `${name} — p. ${page}\n\n${text.trim()}`;
}

export function searchMatches(text, query) {
  const haystack = text.normalize('NFKC').toLocaleLowerCase();
  const needle = query.trim().normalize('NFKC').toLocaleLowerCase();
  if (!needle) return [];
  const matches = [];
  let offset = 0;
  while ((offset = haystack.indexOf(needle, offset)) !== -1 && matches.length < 500) {
    matches.push({ index: offset, excerpt: haystack.slice(Math.max(0, offset - 30), offset + needle.length + 65) });
    offset += needle.length;
  }
  return matches;
}
