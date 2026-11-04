const { contextBridge, ipcRenderer, webUtils } = require('electron');

function subscribe(channel, callback) {
  if (typeof callback !== 'function') throw new TypeError('A callback is required');
  const listener = (_event, value) => callback(value);
  ipcRenderer.on(channel, listener);
  return () => ipcRenderer.removeListener(channel, listener);
}

contextBridge.exposeInMainWorld('desktop', {
  openPdf: () => ipcRenderer.invoke('pdf:open'),
  reopenPdf: id => ipcRenderer.invoke('pdf:reopen', id),
  openDroppedPdf: file => ipcRenderer.invoke('pdf:drop', webUtils.getPathForFile(file)),
  getLibrary: () => ipcRenderer.invoke('library:get'),
  saveReadingState: (id, state) => ipcRenderer.invoke('library:save-state', id, state),
  copyText: text => ipcRenderer.invoke('clipboard:text', text),
  copyImage: dataURL => ipcRenderer.invoke('clipboard:image', dataURL),
  saveImage: (dataURL, suggestedName) => ipcRenderer.invoke('image:save', dataURL, suggestedName),
  toggleFullscreen: () => ipcRenderer.invoke('window:fullscreen'),
  onOpenPdf: callback => subscribe('pdf:opened', callback),
  onOpenError: callback => subscribe('pdf:error', callback),
  onCommand: callback => subscribe('app:command', callback),
});
