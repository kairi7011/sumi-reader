const { app, BrowserWindow, Menu, dialog, ipcMain, clipboard, ClipboardItem, nativeImage, protocol, session } = require('electron');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { APP_URL, APP_ORIGIN, MAX_PDF_BYTES, resolveAsset, sanitizeState, parsePngDataURL } = require('./policy.cjs');

protocol.registerSchemesAsPrivileged([{ scheme: 'app', privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true, stream: true } }]);
app.setName('Sumi');
const root = path.resolve(__dirname, '..');
let mainWindow;
let library = { version: 1, recent: [], lastOpenedId: null };
let writeQueue = Promise.resolve();
let pendingWrites = 0;
const pendingFiles = [];
let rendererReady = false;
let openDialogPromise = null;
const CSP = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data: blob:; connect-src 'self' blob:; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'none'";
const MIME = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.woff': 'font/woff', '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.otf': 'font/otf', '.wasm': 'application/wasm', '.pdf': 'application/pdf', '.bcmap': 'application/octet-stream' };

function storePath() { return path.join(app.getPath('userData'), 'library.json'); }

async function loadLibrary() {
  try {
    const data = JSON.parse(await fs.readFile(storePath(), 'utf8'));
    if (data.version === 1 && Array.isArray(data.recent)) {
      library.recent = data.recent.filter(item => typeof item.id === 'string' && typeof item.path === 'string' && typeof item.name === 'string').slice(0, 40);
      library.lastOpenedId = typeof data.lastOpenedId === 'string' ? data.lastOpenedId : null;
    }
  } catch (error) {
    if (error.code !== 'ENOENT') console.error('Could not restore reading history:', error.message);
  }
}

function persistLibrary() {
  const contents = JSON.stringify(library);
  pendingWrites++;
  writeQueue = writeQueue.catch(() => {}).then(async () => {
    await fs.mkdir(app.getPath('userData'), { recursive: true });
    const temporary = `${storePath()}.tmp`;
    await fs.writeFile(temporary, contents, { mode: 0o600 });
    await fs.rename(temporary, storePath());
  }).finally(() => { pendingWrites--; });
  return writeQueue;
}

async function drainWrites() {
  while (pendingWrites > 0) await writeQueue.catch(error => console.error('Could not save reading history:', error.message));
}

function publicEntry(entry) {
  const { path: _privatePath, ...publicData } = entry;
  return publicData;
}

async function readPdf(filename) {
  if (typeof filename !== 'string' || filename.length > 4096 || path.extname(filename).toLowerCase() !== '.pdf') throw new Error('PDFファイルを選んでください。');
  let canonical;
  let stats;
  try {
    canonical = await fs.realpath(filename);
    stats = await fs.stat(canonical);
  } catch {
    throw new Error('ファイルが見つかりません。移動した場合は、もう一度開いてください。');
  }
  if (!stats.isFile()) throw new Error('PDFファイルを選んでください。');
  if (stats.size > MAX_PDF_BYTES) throw new Error('このPDFは250 MBを超えています。小さなファイルに分けて開いてください。');
  const data = await fs.readFile(canonical);
  if (data.length > MAX_PDF_BYTES) throw new Error('このPDFは250 MBを超えています。');
  if (!data.subarray(0, 1024).includes(Buffer.from('%PDF-'))) throw new Error('PDF形式として読み込めませんでした。');
  const id = crypto.createHash('sha256').update(canonical).digest('hex');
  const previous = library.recent.find(item => item.id === id);
  const entry = { id, path: canonical, name: path.basename(canonical), size: stats.size, modifiedAt: stats.mtimeMs, openedAt: Date.now(), state: previous?.state || {} };
  library.recent = [entry, ...library.recent.filter(item => item.id !== id)].slice(0, 40);
  library.lastOpenedId = id;
  await persistLibrary();
  app.addRecentDocument(canonical);
  return { ...publicEntry(entry), data: new Uint8Array(data) };
}

async function choosePdf() {
  if (openDialogPromise) return openDialogPromise;
  openDialogPromise = choosePdfOnce();
  try { return await openDialogPromise; }
  finally { openDialogPromise = null; }
}

async function choosePdfOnce() {
  if (!mainWindow) {
    createWindow();
    await new Promise(resolve => mainWindow.webContents.once('did-finish-load', resolve));
  }
  const choice = await dialog.showOpenDialog(mainWindow, { title: 'PDFを開く', properties: ['openFile'], filters: [{ name: 'PDF', extensions: ['pdf'] }] });
  return choice.canceled ? null : readPdf(choice.filePaths[0]);
}

async function deliverPdf(filename) {
  if (!rendererReady) { pendingFiles.push(filename); return; }
  try {
    const document = await readPdf(filename);
    mainWindow.webContents.send('pdf:opened', document);
    mainWindow.show();
  } catch (error) { mainWindow.webContents.send('pdf:error', error.message); }
}

function trustedSender(event) {
  return mainWindow && !mainWindow.isDestroyed() && event.sender === mainWindow.webContents && event.senderFrame === mainWindow.webContents.mainFrame && event.senderFrame.url === APP_URL;
}

function handle(channel, callback) {
  ipcMain.handle(channel, (event, ...args) => {
    if (!trustedSender(event)) throw new Error('Untrusted IPC sender');
    return callback(...args);
  });
}

function checkedImage(dataURL) {
  const bytes = parsePngDataURL(dataURL);
  const image = nativeImage.createFromBuffer(bytes);
  const size = image.getSize();
  if (image.isEmpty() || size.width > 16000 || size.height > 16000) throw new Error('画像を読み込めませんでした。');
  return { image, bytes };
}

function registerIPC() {
  handle('pdf:open', choosePdf);
  handle('pdf:drop', filename => readPdf(filename));
  handle('pdf:reopen', id => {
    const entry = library.recent.find(item => item.id === id);
    if (!entry) throw new Error('履歴にあるPDFを選んでください。');
    return readPdf(entry.path);
  });
  handle('library:get', () => ({ recent: library.recent.map(publicEntry), lastOpenedId: library.lastOpenedId }));
  handle('library:save-state', async (id, state) => {
    const entry = library.recent.find(item => item.id === id);
    if (!entry) throw new Error('このPDFは開かれていません。');
    entry.state = sanitizeState(state);
    await persistLibrary();
  });
  handle('clipboard:text', text => {
    if (typeof text !== 'string' || Buffer.byteLength(text) > 8 * 1024 * 1024) throw new Error('コピーする文章が長すぎます。');
    return clipboard.writeText(text);
  });
  handle('clipboard:image', dataURL => {
    const { bytes } = checkedImage(dataURL);
    return clipboard.write([new ClipboardItem({ 'image/png': new Blob([bytes], { type: 'image/png' }) })]);
  });
  handle('image:save', async (dataURL, suggestedName) => {
    const { bytes } = checkedImage(dataURL);
    const basename = typeof suggestedName === 'string' ? path.basename(suggestedName).replace(/[\x00-\x1f]/g, '').slice(0, 180) : 'Sumi-capture.png';
    const choice = await dialog.showSaveDialog(mainWindow, { title: '画像を保存', defaultPath: basename.endsWith('.png') ? basename : `${basename}.png`, filters: [{ name: 'PNG画像', extensions: ['png'] }] });
    if (choice.canceled || !choice.filePath) return false;
    await fs.writeFile(choice.filePath, bytes);
    return true;
  });
  handle('window:fullscreen', () => { mainWindow.setFullScreen(!mainWindow.isFullScreen()); });
}

function createMenu() {
  const command = value => () => mainWindow?.webContents.send('app:command', value);
  Menu.setApplicationMenu(Menu.buildFromTemplate([
    { label: 'Sumi', submenu: [{ role: 'about', label: 'Sumiについて' }, { type: 'separator' }, { role: 'hide', label: 'Sumiを隠す' }, { role: 'hideOthers', label: 'ほかを隠す' }, { role: 'unhide', label: 'すべて表示' }, { type: 'separator' }, { role: 'quit', label: 'Sumiを終了' }] },
    { label: 'ファイル', submenu: [{ label: 'PDFを開く…', accelerator: 'CmdOrCtrl+O', click: async () => { try { const document = await choosePdf(); if (document) mainWindow.webContents.send('pdf:opened', document); } catch (error) { mainWindow.webContents.send('pdf:error', error.message); } } }, { type: 'separator' }, { role: 'close', label: 'ウインドウを閉じる' }] },
    { label: '編集', submenu: [{ role: 'undo', label: '元に戻す' }, { role: 'redo', label: 'やり直す' }, { type: 'separator' }, { role: 'cut', label: 'カット' }, { role: 'copy', label: 'コピー' }, { role: 'paste', label: 'ペースト' }, { role: 'selectAll', label: 'すべて選択' }, { type: 'separator' }, { label: 'PDF内を検索', accelerator: 'CmdOrCtrl+F', click: command('find') }] },
    { label: '表示', submenu: [{ label: '集中モード', accelerator: 'CmdOrCtrl+Shift+F', click: command('focus') }, { label: '範囲を画像でコピー', accelerator: 'CmdOrCtrl+Shift+X', click: command('capture') }, { type: 'separator' }, { role: 'togglefullscreen', label: 'フルスクリーン' }, ...(!app.isPackaged ? [{ type: 'separator' }, { role: 'toggleDevTools' }] : [])] },
    { label: 'ウインドウ', submenu: [{ role: 'minimize', label: 'しまう' }, { role: 'zoom', label: '拡大／縮小' }, { type: 'separator' }, { role: 'front', label: 'すべてを手前に移動' }] },
  ]));
}

function createWindow() {
  rendererReady = false;
  mainWindow = new BrowserWindow({
    width: 1380, height: 940, minWidth: 860, minHeight: 620, title: 'Sumi', backgroundColor: '#eeede8', show: false,
    titleBarStyle: 'hiddenInset', trafficLightPosition: { x: 20, y: 22 },
    webPreferences: { preload: path.join(__dirname, 'preload.cjs'), sandbox: true, contextIsolation: true, nodeIntegration: false, webSecurity: true, spellcheck: false },
  });
  mainWindow.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  mainWindow.webContents.on('will-navigate', event => event.preventDefault());
  mainWindow.webContents.on('will-attach-webview', event => event.preventDefault());
  mainWindow.webContents.once('did-finish-load', () => {
    rendererReady = true;
    mainWindow.show();
    for (const filename of pendingFiles.splice(0)) void deliverPdf(filename);
  });
  let closing = false;
  mainWindow.on('close', event => {
    if (closing || !pendingWrites) return;
    event.preventDefault();
    closing = true;
    const window = mainWindow;
    void drainWrites().then(() => { if (!window.isDestroyed()) window.close(); });
  });
  mainWindow.on('closed', () => { mainWindow = null; rendererReady = false; });
  void mainWindow.loadURL(APP_URL);
  createMenu();
}

app.on('open-file', (event, filename) => {
  event.preventDefault();
  if (app.isReady() && !mainWindow) createWindow();
  void deliverPdf(filename);
});

app.whenReady().then(async () => {
  await loadLibrary();
  protocol.handle('app', async request => {
    try {
      if (request.method !== 'GET' && request.method !== 'HEAD') return new Response('Method not allowed', { status: 405 });
      const filename = resolveAsset(root, request.url);
      const canonical = await fs.realpath(filename);
      if (!canonical.startsWith(`${root}${path.sep}`)) return new Response('Forbidden', { status: 403 });
      resolveAsset(root, `${APP_ORIGIN}/${path.relative(root, canonical).split(path.sep).map(encodeURIComponent).join('/')}`);
      return new Response(request.method === 'HEAD' ? null : await fs.readFile(canonical), { headers: { 'Content-Type': MIME[path.extname(filename)] || 'application/octet-stream', 'Content-Security-Policy': CSP, 'X-Content-Type-Options': 'nosniff' } });
    } catch { return new Response('Not found', { status: 404 }); }
  });
  session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  session.defaultSession.setPermissionCheckHandler(() => false);
  session.defaultSession.webRequest.onBeforeRequest({ urls: ['<all_urls>'] }, (details, callback) => {
    const allowed = details.url.startsWith(`${APP_ORIGIN}/`) || details.url.startsWith('devtools://') || details.url.startsWith('blob:app://sumi/');
    callback({ cancel: !allowed });
  });
  session.defaultSession.on('will-download', event => event.preventDefault());
  registerIPC();
  createWindow();
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
}).catch(error => { console.error(error); app.quit(); });

app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
let quitting = false;
app.on('before-quit', event => {
  if (quitting || !pendingWrites) return;
  event.preventDefault();
  quitting = true;
  void drainWrites().then(() => app.quit());
});
