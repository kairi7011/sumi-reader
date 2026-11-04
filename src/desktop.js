const tauri = window.__TAURI__;

if (tauri && !window.desktop) {
  const invoke = tauri.core.invoke;
  const subscriptions = [];
  let openIntent = 0;
  let pendingOpens = 0;
  const errors = new Set();
  const hydrate = async metadata => metadata ? { ...metadata, data: await invoke('read_pdf', { id: metadata.id }) } : null;
  function subscribe(name, callback) {
    const pending = tauri.event.listen(name, async event => {
      try { await callback(event.payload); }
      catch (error) { for (const handler of errors) handler(String(error.message || error)); }
    });
    subscriptions.push(pending);
    return () => pending.then(unlisten => unlisten());
  }
  async function requestDocument(command, args) {
    const intent = ++openIntent;
    const document = await hydrate(await invoke(command, args));
    return intent === openIntent ? document : null;
  }
  window.desktop = Object.freeze({
    native: 'tauri',
    openPdf: () => requestDocument('open_pdf'),
    reopenPdf: id => requestDocument('reopen_pdf', { id }),
    get hasPendingOpen() { return pendingOpens > 0; },
    getLibrary: () => invoke('get_library'),
    readBookBytes: id => invoke('read_pdf', { id }),
    saveReadingState: (id, state) => invoke('save_reading_state', { id, state }),
    savePreferences: preferences => invoke('save_preferences', { preferences }),
    setBookTitle: (id, title) => invoke('set_book_title', { id, title }),
    copyText: text => invoke('copy_text', { text }),
    copyImage: dataUrl => invoke('copy_image', { dataUrl }),
    saveImage: (dataUrl, suggestedName) => invoke('save_image', { dataUrl, suggestedName }),
    toggleFullscreen: () => invoke('toggle_fullscreen'),
    onOpenPdf: callback => subscribe('pdf-opened', async metadata => {
      const intent = ++openIntent;
      pendingOpens++;
      try {
        const document = await hydrate(metadata);
        if (intent === openIntent) await callback(document);
      } finally { pendingOpens--; }
    }),
    onOpenError: callback => { errors.add(callback); return subscribe('pdf-error', callback); },
    onFileDrag: callback => subscribe('file-drag', callback),
    onCommand: callback => subscribe('app-command', callback),
    ready: async () => { await Promise.all(subscriptions); await invoke('frontend_ready'); },
  });
}
