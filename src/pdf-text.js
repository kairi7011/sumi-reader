export async function readPageText(page) {
  // WebKit 18 can read streams but cannot asynchronously iterate them.
  const reader = page.streamTextContent().getReader();
  const content = { items: [], styles: Object.create(null), lang: null };
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      content.lang ??= value.lang;
      Object.assign(content.styles, value.styles);
      content.items.push(...value.items);
    }
    return content;
  } finally { reader.releaseLock(); }
}
