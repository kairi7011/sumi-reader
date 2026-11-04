const tauri = window.__TAURI__;

if (tauri && !window.desktop) {
  const invoke = tauri.core.invoke;
  const subscriptions = [];
  const errors = new Set();
  const hydrate = async metadata => metadata ? { ...metadata, data: await invoke('read_pdf', { id: metadata.id }) } : null;
  function subscribe(name, callback, transform = value => value) {
    const pending = tauri.event.listen(name, async event => {
      try { await callback(await transform(event.payload)); }
      catch (error) { for (const handler of errors) handler(String(error.message || error)); }
    });
    subscriptions.push(pending);
    return () => pending.then(unlisten => unlisten());
  }
  window.desktop = Object.freeze({
    native: 'tauri',
    openPdf: async () => hydrate(await invoke('open_pdf')),
    reopenPdf: async id => hydrate(await invoke('reopen_pdf', { id })),
    getLibrary: () => invoke('get_library'),
    saveReadingState: (id, state) => invoke('save_reading_state', { id, state }),
    copyText: text => invoke('copy_text', { text }),
    copyImage: dataUrl => invoke('copy_image', { dataUrl }),
    saveImage: (dataUrl, suggestedName) => invoke('save_image', { dataUrl, suggestedName }),
    toggleFullscreen: () => invoke('toggle_fullscreen'),
    onOpenPdf: callback => subscribe('pdf-opened', callback, hydrate),
    onOpenError: callback => { errors.add(callback); return subscribe('pdf-error', callback); },
    onFileDrag: callback => subscribe('file-drag', callback),
    onCommand: callback => subscribe('app-command', callback),
    ready: async () => { await Promise.all(subscriptions); await invoke('frontend_ready'); },
  });
}
