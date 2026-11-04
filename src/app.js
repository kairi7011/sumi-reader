import './desktop.js';
import { PaperTurn } from './page-turn.js';
import { PageMap } from './page-map.js';
import { PageZoom } from './page-zoom.js';
import { deriveBookTitle } from './book-title.js';
import { readPageText } from './pdf-text.js';
import * as pdfjs from '../node_modules/pdfjs-dist/legacy/build/pdf.mjs';
import { renderIcons, icon } from './icons.js';
import { clamp, normalizeState, visiblePages, cropRect, pageText, textSegments, matchingSegments, citation, searchMatches } from './model.js';

pdfjs.GlobalWorkerOptions.workerSrc = new URL('../node_modules/pdfjs-dist/legacy/build/pdf.worker.mjs', import.meta.url).href;
const $ = id => document.getElementById(id);
const desktop = window.desktop;
const asset = path => new URL(`../node_modules/pdfjs-dist/${path}/`, import.meta.url).href;
let pdf = null;
let book = null;
let state = normalizeState();
let library = [];
let outline = [];
let sidebarTab = 'library';
let opening = 0;
let rendering = 0;
let loadingTask = null;
let pendingBatch = null;
let displayedPage = 0;
let displayedMode = null;
let displayedZoom = 1;
const paperTurn = new PaperTurn(document.getElementById('reading-area'));
let pageProxies = [];
let textLayers = [];
let toastTimer;
let searchTimer;
let searchToken = 0;
let searchResults = [];
let resultIndex = -1;
let captureMode = false;
let renderReady = false;
let turnGesture = null;
const pageMap = new PageMap({
  getDocument: () => pdf,
  getPage: () => displayedPage || state.page,
  getProtectedPages: () => pdf ? [...visiblePages(displayedPage || state.page, pdf.numPages, displayedMode || state.mode), ...visiblePages(state.page, pdf.numPages, state.mode)] : [],
  onNavigate: page => navigate(page),
});
const pageZoom = new PageZoom({ reader: $('reader'), getDocument: () => pdf, notify: toast, onChange: syncControls });
let preferences = { theme: 'paper', mode: 'single', direction: 'ltr' };
let preferencesReady = false;
let preferenceSave = Promise.resolve();
let titleJobRunning = false;
const titleAttempts = new Set();
let clips = [];
let selectedExcerpt = null;
const textCache = new Map();
const selectionToolbar = document.createElement('div');
selectionToolbar.id = 'selection-toolbar';
selectionToolbar.hidden = true;
const selectionCopy = document.createElement('button');
selectionCopy.innerHTML = `${icon('copy')}<span>引用してコピー</span>`;
selectionToolbar.append(selectionCopy);
document.body.append(selectionToolbar);

function toast(message, error = false) {
  clearTimeout(toastTimer);
  $('toast').textContent = message;
  $('toast').dataset.error = String(error);
  $('toast').hidden = false;
  toastTimer = setTimeout(() => { $('toast').hidden = true; }, error ? 5500 : 2800);
}

function busy(message) {
  $('busy').hidden = !message;
  $('busy-label').textContent = message || '';
  $('reader').setAttribute('aria-busy', String(Boolean(message)));
}

function showError(message) {
  $('reader-status').textContent = message;
  $('reader-status').dataset.error = 'true';
  $('reader-status').hidden = false;
  toast(message, true);
}

async function action(button, task, success) {
  if (button?.dataset.busy === 'true') return;
  if (button) { button.dataset.busy = 'true'; button.setAttribute('aria-busy', 'true'); }
  try {
    const result = await task();
    if (success && result !== false) toast(success);
    return result;
  } catch (error) {
    toast(error.message || '操作を完了できませんでした。もう一度お試しください。', true);
  } finally {
    if (button) { button.dataset.busy = 'false'; button.setAttribute('aria-busy', 'false'); }
  }
}

function persist() {
  if (!book || !pdf) return;
  const id = book.id;
  const snapshot = { ...structuredClone(state), zoom: 1 };
  desktop.saveReadingState(id, snapshot).catch(() => toast('読書位置を保存できませんでした。', true));
  const recent = library.find(item => item.id === id);
  if (recent) recent.state = snapshot;
}

function syncControls() {
  for (const control of document.querySelectorAll('[data-needs-document]')) control.disabled = !pdf;
  $('prev-page').disabled = !pdf || state.page <= 1;
  $('next-page').disabled = !pdf || state.page + (state.mode === 'spread' ? 1 : 0) >= pdf.numPages;
  $('page-input').value = state.page;
  $('page-input').max = pdf?.numPages || 1;
  $('page-count').textContent = pdf?.numPages || '—';
  $('page-progress').value = pdf ? Math.round(state.page / pdf.numPages * 100) : 0;
  $('progress-label').textContent = `${$('page-progress').value}%`;
  $('zoom-value').textContent = `${Math.round(pageZoom.factor * 100)}%`;
  $('zoom-out').disabled = !renderReady || pageZoom.factor <= 1;
  $('zoom-in').disabled = !renderReady || pageZoom.factor >= 4;
  $('page-map-toggle').disabled = !pdf;
  $('capture-region').disabled = !pdf || !renderReady;
  $('copy-page').disabled = !pdf || !renderReady;
  $('bookmark-toggle').disabled = !pdf || !renderReady;
  $('fit-page').setAttribute('aria-pressed', String(pageZoom.factor === 1));
  const marked = state.bookmarks.includes(state.page);
  $('bookmark-toggle').setAttribute('aria-pressed', String(marked));
  $('bookmark-toggle').setAttribute('aria-label', marked ? 'しおりを外す' : 'しおりを付ける');
  $('bookmark-toggle').querySelector('span:not([data-icon])').textContent = marked ? 'しおりを外す' : 'しおりを付ける';
  $('layout-select').value = state.mode;
  $('layout-single').setAttribute('aria-pressed', String(state.mode === 'single'));
  $('layout-spread').setAttribute('aria-pressed', String(state.mode === 'spread'));
  $('layout-toggle').setAttribute('aria-pressed', String(state.mode === 'spread'));
  $('layout-toggle').title = state.mode === 'spread' ? '1ページで読む' : '見開きで読む';
  $('layout-toggle').setAttribute('aria-label', $('layout-toggle').title);
  $('direction-select').value = state.direction;
  $('theme-select').value = state.theme;
  document.body.dataset.theme = state.theme;
  document.body.dataset.direction = state.direction;
  $('pages').dataset.direction = state.direction;
  $('footer-status').textContent = pdf ? (renderReady ? '読書位置を自動保存' : `${state.page}ページを準備`) : 'PDFを開いて読み始める';
  $('document-title').textContent = book?.name || 'Sumi';
  $('document-title').title = book?.filename || '';
  $('reader').dataset.documentId = book?.id || '';
  $('document-meta').textContent = pdf ? `${pdf.numPages} ページ · このMacで読む` : 'あなたのための読書室';
}

function sidebarEmpty(text) {
  const p = document.createElement('p');
  p.className = 'sidebar-empty';
  p.textContent = text;
  $('sidebar-content').append(p);
}

function renderSidebar() {
  $('sidebar-content').replaceChildren();
  document.querySelectorAll('[data-sidebar-tab]').forEach(button => button.setAttribute('aria-selected', String(button.dataset.sidebarTab === sidebarTab)));
  if (sidebarTab === 'library') {
    const heading = document.createElement('p');
    heading.className = 'sidebar-eyebrow';
    heading.textContent = '最近読んだ本';
    $('sidebar-content').append(heading);
    if (!library.length) return sidebarEmpty('開いた本がここに並びます。');
    for (const item of library) {
      const button = document.createElement('button');
      button.className = 'library-item';
      button.setAttribute('aria-current', String(item.id === book?.id));
      const content = document.createElement('span');
      const name = document.createElement('strong');
      name.textContent = (item.title || item.name).replace(/\.pdf$/i, '');
      name.title = item.name;
      const meta = document.createElement('small');
      meta.textContent = `${item.state?.page || 1} ページから · ${(item.size / 1048576).toFixed(1)} MB`;
      content.append(name, meta);
      button.innerHTML = icon('book-open');
      button.append(content);
      button.addEventListener('click', () => action(button, async () => {
        const doc = await desktop.reopenPdf(item.id);
        if (doc) await openDocument(doc);
      }));
      $('sidebar-content').append(button);
    }
  } else if (sidebarTab === 'bookmarks') {
    if (!pdf) return sidebarEmpty('本を開くと、しおりを付けられます。');
    if (!state.bookmarks.length) return sidebarEmpty('残しておきたいページで、しおりボタンを押してください。');
    for (const page of state.bookmarks) {
      const button = document.createElement('button');
      button.className = `bookmark-item${page === state.page ? ' active' : ''}`;
      button.textContent = `しおり　${page} ページ`;
      button.addEventListener('click', () => navigate(page));
      $('sidebar-content').append(button);
    }
  } else {
    if (!outline.length) return sidebarEmpty('このPDFには目次が登録されていません。');
    function append(items, depth = 0) {
      for (const item of items) {
        const button = document.createElement('button');
        button.className = 'outline-item';
        button.style.paddingLeft = `${10 + Math.min(depth, 6) * 12}px`;
        button.textContent = item.title;
        button.disabled = !item.dest;
        button.addEventListener('click', () => action(button, async () => {
          const documentAtClick = pdf;
          const dest = typeof item.dest === 'string' ? await documentAtClick.getDestination(item.dest) : item.dest;
          if (!dest || documentAtClick !== pdf) return;
          const page = typeof dest[0] === 'number' ? dest[0] : await documentAtClick.getPageIndex(dest[0]);
          if (documentAtClick === pdf) await navigate(page + 1);
        }));
        $('sidebar-content').append(button);
        append(item.items || [], depth + 1);
      }
    }
    append(outline);
  }
}

async function refreshLibrary() {
  const data = await desktop.getLibrary();
  library = data.recent;
  if (!preferencesReady) { preferences = { ...preferences, ...data.preferences }; preferencesReady = true; Object.assign(state, preferences); syncControls(); }
  renderSidebar();
  setTimeout(updateLibraryTitles, 500);
  return data;
}

async function updateLibraryTitles() {
  if (titleJobRunning || !desktop.readBookBytes) return;
  if (pdf && !renderReady) { setTimeout(updateLibraryTitles, 500); return; }
  const item = library.find(item => item.id !== book?.id && (!item.title || item.title === item.name) && !titleAttempts.has(`${item.id}:${item.modifiedAt}`));
  if (!item) return;
  titleAttempts.add(`${item.id}:${item.modifiedAt}`);
  titleJobRunning = true;
  let task, timeout;
  try {
    const data = await desktop.readBookBytes(item.id);
    task = pdfjs.getDocument({ data: new Uint8Array(data), cMapUrl: asset('cmaps'), cMapPacked: true, isEvalSupported: false, enableScripting: false });
    task.onPassword = () => task.destroy().catch(() => {});
    timeout = setTimeout(() => task.destroy().catch(() => {}), 8000);
    const document = await task.promise;
    const title = await deriveBookTitle(document, item.name);
    if (title) {
      await desktop.setBookTitle(item.id, title);
      const current = library.find(book => book.id === item.id && book.modifiedAt === item.modifiedAt);
      if (current) { current.title = title; renderSidebar(); }
    }
  } catch { /* A missing or locked recent file must not interrupt the current book. */ }
  finally {
    clearTimeout(timeout);
    await task?.destroy().catch(() => {});
    titleJobRunning = false;
    setTimeout(updateLibraryTitles, 500);
  }
}

function askPassword(updatePassword, reason) {
  $('password-error').textContent = 'パスワードが違うようです。もう一度入力してください。';
  $('password-error').hidden = reason !== pdfjs.PasswordResponses.INCORRECT_PASSWORD;
  $('password-input').value = '';
  if (!$('password-dialog').open) $('password-dialog').showModal();
  $('password-input').focus();
  $('password-form').onsubmit = event => {
    event.preventDefault();
    const password = $('password-input').value;
    $('password-input').value = '';
    $('password-dialog').close();
    updatePassword(password);
  };
}

async function openDocument(document) {
  if (!document) return;
  const token = ++opening;
  pageMap.reset();
  pageZoom.reset();
  closePanels();
  $('password-dialog').close();
  $('password-form').onsubmit = null;
  $('password-input').value = '';
  ++searchToken;
  ++rendering;
  renderReady = false;
  setCapture(false);
  selectionToolbar.hidden = true;
  if (book && pdf) await desktop.saveReadingState(book.id, { ...state, zoom: 1 }).catch(() => {});
  if (token !== opening) return;
  cancelRender(true);
  displayedPage = 0;
  const previousTask = loadingTask;
  loadingTask = null;
  await previousTask?.destroy().catch(() => {});
  if (token !== opening) return;
  pdf = null;
  book = null;
  outline = [];
  textCache.clear();
  searchResults = [];
  resultIndex = -1;
  $('search-status').textContent = '';
  $('search-input').value = '';
  $('reader-status').hidden = true;
  $('pages').replaceChildren();
  syncControls();
  busy('本を開いています…');
  const task = pdfjs.getDocument({
    data: new Uint8Array(document.data),
    cMapUrl: asset('cmaps'), cMapPacked: true,
    standardFontDataUrl: asset('standard_fonts'),
    wasmUrl: asset('wasm'), iccUrl: asset('iccs'),
    isEvalSupported: false,
    enableScripting: false,
    useSystemFonts: true,
  });
  loadingTask = task;
  task.onPassword = (update, reason) => { if (token === opening) askPassword(update, reason); };
  task.onProgress = progress => {
    if (token === opening && progress.total) busy(`本を開いています… ${Math.min(100, Math.round(progress.loaded / progress.total * 100))}%`);
  };
  try {
    const loaded = await task.promise;
    if (token !== opening) { await loaded.destroy(); return; }
    pdf = loaded;
    if (!preferencesReady) await refreshLibrary();
    if (token !== opening) return;
    const title = await deriveBookTitle(loaded, document.name).catch(() => null);
    if (token !== opening) return;
    book = { id: document.id, name: title || document.title || document.name.replace(/\.pdf$/i, ''), filename: document.name };
    if (title && title !== document.title) await desktop.setBookTitle(document.id, title).catch(() => {});
    if (token !== opening) return;
    state = normalizeState({ ...document.state, ...preferences }, pdf.numPages);
    state.fit = 'page';
    $('welcome').hidden = true;
    $('reader').hidden = false;
    syncControls();
    await renderPages();
    if (token !== opening) return;
    outline = await loaded.getOutline() || [];
    if (token !== opening) return;
    await refreshLibrary();
    renderSidebar();
    persist();
  } catch (error) {
    if (token !== opening) return;
    console.error('PDF open failed', error);
    $('welcome').hidden = false;
    $('reader').hidden = true;
    showError(error.name === 'InvalidPDFException' ? 'PDFを読み込めませんでした。ファイルが壊れていないか確認してください。' : '本を開けませんでした。別のPDFをお試しください。');
  } finally {
    if (token === opening) busy(null);
  }
}

function disposeBatch(batch) {
  if (!batch || batch.disposed) return;
  batch.disposed = true;
  batch.tasks.forEach(task => task.cancel());
  batch.layers.forEach(layer => layer.cancel());
  batch.node.remove();
  batch.node.querySelectorAll('canvas').forEach(canvas => { canvas.width = canvas.height = 0; });
  for (const page of batch.proxies) {
    if (!pageProxies.includes(page) && !pendingBatch?.proxies.includes(page)) page.cleanup();
  }
}

function clearTurnGesture(gesture) {
  if (!gesture || turnGesture !== gesture) return;
  const reader = $('reader');
  if (reader.hasPointerCapture(gesture.pointerId)) reader.releasePointerCapture(gesture.pointerId);
  turnGesture = null;
  delete reader.dataset.grabbing;
}

function cancelRender(all = false, keepGesture = null) {
  paperTurn.finish();
  if (turnGesture !== keepGesture) clearTurnGesture(turnGesture);
  const pending = pendingBatch;
  pendingBatch = null;
  disposeBatch(pending);
  if (all) {
    textLayers.forEach(layer => layer.cancel());
    textLayers = [];
    const previous = pageProxies;
    pageProxies = [];
    previous.forEach(page => page.cleanup());
  }
}

async function renderPages(turn = false, gesture = null) {
  if (!pdf) return;
  clearTimeout(resizeTimer);
  const token = ++rendering;
  const currentPdf = pdf;
  state.fit = 'page';
  const target = { ...state };
  cancelRender(false, gesture);
  pageZoom.reset();
  renderReady = false;
  setCapture(false);
  selectionToolbar.hidden = true;
  $('reader-status').hidden = true;
  $('reader').dataset.loading = 'true';
  syncControls();
  const loading = setTimeout(() => {
    if (token === rendering) busy('次のページを準備しています…');
  }, 180);
  const node = document.createElement('div');
  node.className = 'pages staging-pages';
  node.dataset.direction = target.direction;
  node.inert = true;
  node.setAttribute('aria-hidden', 'true');
  $('reader').append(node);
  const batch = { node, proxies: [], tasks: [], layers: [] };
  pendingBatch = batch;
  let committed = false;
  try {
    const numbers = visiblePages(target.page, currentPdf.numPages, target.mode);
    batch.proxies = await Promise.all(numbers.map(page => currentPdf.getPage(page)));
    if (token !== rendering) {
      batch.proxies.forEach(page => { if (!pageProxies.includes(page) && !pendingBatch?.proxies.includes(page)) page.cleanup(); });
      return;
    }
    const base = batch.proxies.map(page => page.getViewport({ scale: 1 }));
    const reader = $('reader');
    const style = getComputedStyle(reader);
    const availableWidth = reader.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - (numbers.length - 1) * 18;
    const availableHeight = reader.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
    const fitWidth = availableWidth / base.reduce((sum, page) => sum + page.width, 0);
    const fitHeight = availableHeight / Math.max(...base.map(page => page.height));
    target.zoom = Math.max(Number.EPSILON, Math.min(fitWidth, fitHeight, 3));
    await Promise.all(batch.proxies.map(async (page, index) => {
      const viewport = page.getViewport({ scale: target.zoom });
      const wrapper = document.createElement('div');
      wrapper.className = 'pdf-page';
      wrapper.dataset.pageNumber = numbers[index];
      wrapper.setAttribute('aria-label', `${numbers[index]} ページ`);
      wrapper.style.width = `${viewport.width}px`;
      wrapper.style.height = `${viewport.height}px`;
      wrapper.style.setProperty('--total-scale-factor', viewport.scale);
      const canvas = document.createElement('canvas');
      const outputScale = Math.min(devicePixelRatio || 1, 2, Math.sqrt(14_000_000 / (viewport.width * viewport.height)));
      canvas.width = Math.floor(viewport.width * outputScale);
      canvas.height = Math.floor(viewport.height * outputScale);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      const layerDiv = document.createElement('div');
      layerDiv.className = 'textLayer';
      wrapper.append(canvas, layerDiv);
      node.append(wrapper);
      const task = page.render({ canvasContext: canvas.getContext('2d', { alpha: false }), viewport, transform: [outputScale, 0, 0, outputScale, 0, 0] });
      batch.tasks.push(task);
      await task.promise;
      if (token !== rendering) return;
      const content = await readPageText(page);
      if (token !== rendering) return;
      cacheText(numbers[index], pageText(content));
      const layer = new pdfjs.TextLayer({ textContentSource: content, container: layerDiv, viewport });
      layer.sumiSegments = textSegments(content);
      batch.layers.push(layer);
      await layer.render();
    }));
    if (token !== rendering) return;
    clearTimeout(loading);
    busy(null);
    const previousState = { page: displayedPage, mode: displayedMode, zoom: displayedZoom };
    const previous = { node: $('pages'), tasks: [], layers: textLayers, proxies: pageProxies };
    const direction = turn && displayedMode === target.mode && Math.abs(target.page - displayedPage) === (target.mode === 'spread' ? 2 : 1)
      ? Math.sign(target.page - displayedPage) * (target.direction === 'rtl' ? -1 : 1) : 0;
    // Capture old geometry before the atomic swap. Neither an empty canvas nor half a spread is ever shown.
    let animation;
    if (gesture && direction) {
      gesture.session = paperTurn.createSession(previous.node, node, direction, { cornerY: gesture.cornerY });
      gesture.session.update(gesture.progress, gesture.vertical);
      if (gesture.released) gesture.session.settle(gesture.commit, gesture.velocity);
      animation = gesture.session.finished;
    } else animation = paperTurn.play(previous.node, node, direction).then(() => true);
    previous.node.replaceWith(node);
    node.id = 'pages';
    node.classList.remove('staging-pages');
    node.removeAttribute('aria-hidden');
    if (!direction) node.inert = false;
    pendingBatch = null;
    pageProxies = batch.proxies;
    textLayers = batch.layers;
    committed = true;
    displayedPage = target.page;
    displayedMode = target.mode;
    displayedZoom = target.zoom;
    state.zoom = target.zoom;
    reader.scrollTop = 0;
    reader.scrollLeft = target.direction === 'rtl' ? reader.scrollWidth : 0;
    highlightSearch();
    const accepted = await animation;
    if (!accepted && token === rendering) {
      node.replaceWith(previous.node);
      previous.node.inert = false;
      pageProxies = previous.proxies;
      textLayers = previous.layers;
      displayedPage = state.page = previousState.page;
      displayedMode = state.mode = previousState.mode;
      displayedZoom = state.zoom = previousState.zoom;
      disposeBatch(batch);
    } else disposeBatch(previous);
    clearTurnGesture(gesture);
    if (token !== rendering) return;
    renderReady = true;
    installPageCorners();
    syncControls();
    persist();
    renderSidebar();
  } catch (error) {
    if (token !== rendering || error.name === 'RenderingCancelledException' || error.name === 'AbortException') return;
    state.page = displayedPage || 1;
    state.mode = displayedMode || state.mode;
    state.zoom = displayedZoom;
    renderReady = Boolean(displayedPage && $('pages').querySelector('canvas')?.width);
    syncControls();
    console.error('Page render failed', error);
    showError('このページを表示できませんでした。もう一度めくるか、PDFを開き直してください。');
  } finally {
    clearTimeout(loading);
    clearTurnGesture(gesture);
    if (!committed) { if (pendingBatch === batch) pendingBatch = null; disposeBatch(batch); }
    if (token === rendering) { busy(null); delete $('reader').dataset.loading; }
  }
}

async function navigate(page, gesture = null) {
  if (!pdf) return;
  const next = clamp(Math.floor(Number(page)) || 1, 1, pdf.numPages);
  if (next === state.page && renderReady) return;
  state.page = next;
  syncControls();
  await renderPages(true, gesture);
}

function step(delta) { return navigate(state.page + delta * (state.mode === 'spread' ? 2 : 1)); }

function cacheText(page, text) {
  textCache.delete(page);
  textCache.set(page, text);
  if (textCache.size > 32) textCache.delete(textCache.keys().next().value);
}

async function getText(page, currentPdf = pdf) {
  if (currentPdf === pdf && textCache.has(page)) return textCache.get(page);
  const proxy = await currentPdf.getPage(page);
  const text = pageText(await readPageText(proxy));
  if (currentPdf === pdf) cacheText(page, text);
  if (!pageProxies.includes(proxy) && !pendingBatch?.proxies.includes(proxy)) proxy.cleanup();
  return text;
}

function setPanel(id, show) {
  if (show) { closePanels(); pageZoom.reset(); }
  $(id).hidden = !show;
  $(id === 'sidebar' ? 'toggle-sidebar' : 'clips-toggle').setAttribute('aria-pressed', String(show));
}

function addClip(clip) {
  clips.unshift({ ...clip, id: crypto.randomUUID() });
  clips = clips.slice(0, 24);
  let bytes = 0;
  let pixels = 0;
  clips = clips.filter(item => {
    bytes += item.image?.length || item.text?.length || 0;
    pixels += item.pixels || 0;
    return bytes <= 32_000_000 && pixels <= 24_000_000;
  });
  renderClips();
}

function renderClips() {
  $('clips-list').replaceChildren();
  $('clips-empty').hidden = clips.length > 0;
  $('clips-count').textContent = clips.length;
  $('copy-all-clips').disabled = !clips.some(clip => clip.text);
  for (const clip of clips) {
    const card = document.createElement('article');
    card.className = 'clip-card';
    const meta = document.createElement('div');
    meta.className = 'clip-meta';
    const source = document.createElement('span');
    source.textContent = `${clip.name} · p. ${clip.page}`;
    source.title = source.textContent;
    meta.append(source);
    const preview = document.createElement(clip.image ? 'img' : 'p');
    preview.className = clip.image ? 'clip-preview' : 'clip-text';
    if (clip.image) { preview.src = clip.thumbnail; preview.alt = `${clip.name} ${clip.page}ページの切り抜き`; }
    else preview.textContent = clip.text;
    const actions = document.createElement('div');
    actions.className = 'clip-actions';
    const copy = document.createElement('button');
    copy.className = 'text-button';
    copy.textContent = 'コピー';
    copy.addEventListener('click', () => action(copy, () => clip.image ? desktop.copyImage(clip.image) : desktop.copyText(citation(clip.name, clip.page, clip.text)), 'コピーしました'));
    const remove = document.createElement('button');
    remove.className = 'icon-button';
    remove.innerHTML = icon('close');
    remove.setAttribute('aria-label', '切り抜きを削除');
    remove.addEventListener('click', () => { clips = clips.filter(item => item.id !== clip.id); renderClips(); toast('切り抜きを削除しました'); });
    actions.append(copy);
    if (clip.image) {
      const save = document.createElement('button');
      save.className = 'text-button';
      save.textContent = '保存';
      save.addEventListener('click', () => action(save, () => desktop.saveImage(clip.image, `${clip.name.replace(/\.pdf$/i, '')}-p${clip.page}.png`), '画像を保存しました'));
      actions.append(save);
    }
    actions.append(remove);
    card.append(meta, preview, actions);
    $('clips-list').append(card);
  }
}

async function copyExcerpt(text, page, button, name = book?.name) {
  if (!text?.trim()) return toast('このページにコピーできる文章がありません。「切り抜く」で画像にできます。', true);
  await action(button, async () => {
    await desktop.copyText(citation(name, page, text));
    addClip({ text, page, name });
  }, '書名とページ番号を添えてコピーしました');
}

function setCapture(enabled) {
  if (enabled && !renderReady) { toast('ページの描画が終わってから切り抜けます'); return; }
  captureMode = Boolean(enabled && pdf);
  document.body.dataset.capturing = String(captureMode);
  $('capture-region').setAttribute('aria-pressed', String(captureMode));
  document.querySelectorAll('.capture-overlay').forEach(element => element.remove());
  if (!captureMode) { if (!$('reader-status').dataset.error || $('reader-status').dataset.error === 'false') $('reader-status').hidden = true; return; }
  closePanels();
  selectionToolbar.hidden = true;
  window.getSelection()?.removeAllRanges();
  $('reader-status').dataset.error = 'false';
  $('reader-status').textContent = 'ページの中をドラッグして切り抜く · Escでキャンセル';
  $('reader-status').hidden = false;
  for (const wrapper of document.querySelectorAll('.pdf-page')) {
    const overlay = document.createElement('div');
    overlay.className = 'capture-overlay';
    const rect = document.createElement('div');
    rect.className = 'capture-rect';
    rect.hidden = true;
    overlay.append(rect);
    wrapper.append(overlay);
    let start = null;
    let current = null;
    const point = event => { const bounds = overlay.getBoundingClientRect(); return { x: (event.clientX - bounds.left) * overlay.clientWidth / bounds.width, y: (event.clientY - bounds.top) * overlay.clientHeight / bounds.height }; };
    overlay.addEventListener('pointerdown', event => {
      if (event.button !== 0) return;
      event.preventDefault();
      overlay.setPointerCapture(event.pointerId);
      start = point(event);
      current = null;
      rect.hidden = false;
      Object.assign(rect.style, { left: `${start.x}px`, top: `${start.y}px`, width: '0px', height: '0px' });
    });
    overlay.addEventListener('pointermove', event => {
      if (!start) return;
      current = cropRect(start, point(event), overlay.clientWidth, overlay.clientHeight);
      Object.assign(rect.style, { left: `${current.x}px`, top: `${current.y}px`, width: `${current.width}px`, height: `${current.height}px` });
    });
    overlay.addEventListener('pointercancel', () => { start = null; rect.hidden = true; });
    overlay.addEventListener('pointerup', event => {
      if (!start) return;
      current = cropRect(start, point(event), overlay.clientWidth, overlay.clientHeight);
      start = null;
      if (current.width < 8 || current.height < 8) { rect.hidden = true; toast('もう少し大きくドラッグしてください'); return; }
      const source = wrapper.querySelector('canvas');
      const ratioX = source.width / overlay.clientWidth;
      const ratioY = source.height / overlay.clientHeight;
      const result = document.createElement('canvas');
      const scale = Math.min(1, Math.sqrt(6_000_000 / (current.width * ratioX * current.height * ratioY)));
      result.width = Math.max(1, Math.floor(current.width * ratioX * scale));
      result.height = Math.max(1, Math.floor(current.height * ratioY * scale));
      result.getContext('2d').drawImage(source, current.x * ratioX, current.y * ratioY, current.width * ratioX, current.height * ratioY, 0, 0, result.width, result.height);
      const image = result.toDataURL('image/png');
      const thumbnail = document.createElement('canvas');
      const thumbnailScale = Math.min(1, 440 / Math.max(result.width, result.height));
      thumbnail.width = Math.max(1, Math.round(result.width * thumbnailScale));
      thumbnail.height = Math.max(1, Math.round(result.height * thumbnailScale));
      thumbnail.getContext('2d').drawImage(result, 0, 0, thumbnail.width, thumbnail.height);
      const clip = { image, thumbnail: thumbnail.toDataURL('image/png'), pixels: result.width * result.height, page: Number(wrapper.dataset.pageNumber), name: book.name };
      setCapture(false);
      addClip(clip);
      setPanel('clips-panel', true);
      action($('capture-region'), () => desktop.copyImage(image), '切り抜いた画像をコピーしました');
    });
  }
}

function toggleSearch(show = $('search-panel').hidden) {
  if (!pdf) return;
  $('search-panel').hidden = !show;
  $('search-toggle').setAttribute('aria-pressed', String(show));
  if (show) { $('search-input').focus(); $('search-input').select(); }
  else { ++searchToken; clearTimeout(searchTimer); highlightSearch(); }
}

async function search() {
  if ($('search-panel').hidden) return;
  const query = $('search-input').value.trim();
  const token = ++searchToken;
  const currentPdf = pdf;
  searchResults = [];
  resultIndex = -1;
  $('search-prev').disabled = true;
  $('search-next').disabled = true;
  if (!query || !currentPdf) { $('search-status').textContent = ''; highlightSearch(); return; }
  try {
    for (let page = 1; page <= currentPdf.numPages; page++) {
      if (token !== searchToken || currentPdf !== pdf) return;
      $('search-status').textContent = `検索中 ${page} / ${currentPdf.numPages}`;
      const text = await getText(page, currentPdf);
      if (token !== searchToken || currentPdf !== pdf) return;
      if (searchMatches(text, query).length) searchResults.push(page);
      if (page % 6 === 0) await new Promise(resolve => setTimeout(resolve, 0));
    }
    if (token !== searchToken) return;
    if (!searchResults.length) { $('search-status').textContent = '見つかりませんでした'; highlightSearch(); return; }
    resultIndex = Math.max(0, searchResults.findIndex(page => page >= state.page));
    $('search-prev').disabled = false;
    $('search-next').disabled = false;
    await visitResult(0);
  } catch (error) {
    if (token === searchToken) { $('search-status').textContent = '検索できませんでした'; console.error(error); }
  }
}

async function visitResult(delta) {
  if (!searchResults.length) return;
  resultIndex = (resultIndex + delta + searchResults.length) % searchResults.length;
  $('search-status').textContent = `${resultIndex + 1} / ${searchResults.length} ページ`;
  await navigate(searchResults[resultIndex]);
  highlightSearch();
}

function highlightSearch() {
  const query = $('search-panel').hidden ? '' : $('search-input').value;
  for (const layer of textLayers) {
    const selected = matchingSegments(layer.sumiSegments || [], query);
    layer.textDivs.forEach((span, index) => span.classList.toggle('search-hit', selected.has(index)));
  }
}

function bookmark() {
  if (!pdf || !renderReady) return;
  const exists = state.bookmarks.includes(state.page);
  state.bookmarks = exists ? state.bookmarks.filter(page => page !== state.page) : [...state.bookmarks, state.page].sort((a, b) => a - b);
  syncControls(); renderSidebar(); persist();
  toast(exists ? 'しおりを外しました' : `${state.page}ページにしおりを付けました`);
}

function focusMode() {
  const focused = document.body.dataset.focus !== 'true';
  if (focused) { closePanels(); pageZoom.reset(); setCapture(false); toggleSearch(false); }
  document.body.dataset.focus = String(focused);
  $('focus-toggle').setAttribute('aria-pressed', String(focused));
  if (pdf) $('reader').focus({ preventScroll: true });
  toast(focused ? '集中モード · Escで戻る' : '通常表示に戻りました');
}

function closeTools() {
  if ($('reader-tools').contains(document.activeElement)) $('tools-toggle').focus({ preventScroll: true });
  $('reader-tools').hidden = true;
  $('tools-toggle').setAttribute('aria-expanded', 'false');
}

function closePanels() {
  for (const id of ['sidebar', 'clips-panel']) {
    $(id).hidden = true;
    $(id === 'sidebar' ? 'toggle-sidebar' : 'clips-toggle').setAttribute('aria-pressed', 'false');
  }
  closeTools();
}

function showPageMap() {
  if (!pdf) return;
  pageZoom.reset(); closePanels();
  pageMap.open();
  $('page-map-toggle').setAttribute('aria-expanded', String(pageMap.dialog.open));
}

function zoom(delta) {
  if (pdf && renderReady) pageZoom.zoom(delta);
}

function installPageCorners() {
  $('pages').querySelectorAll('.page-corner').forEach(corner => corner.remove());
  const pages = [...$('pages').querySelectorAll('.pdf-page')].sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left);
  for (const [index, side] of [[0, 'left'], [pages.length - 1, 'right']]) {
    const delta = (side === 'right' ? 1 : -1) * (state.direction === 'rtl' ? -1 : 1);
    const next = state.page + delta * (state.mode === 'spread' ? 2 : 1);
    if (!pages[index] || next < 1 || next > pdf.numPages) continue;
    const corner = document.createElement('button');
    corner.className = `page-corner ${side}`;
    corner.dataset.delta = delta;
    corner.setAttribute('aria-label', delta > 0 ? '紙の端をつかんで次のページへ' : '紙の端をつかんで前のページへ');
    corner.title = 'ドラッグでめくる · 離すと確定、戻すと取り消し';
    corner.addEventListener('click', event => { if (event.detail === 0) step(delta); });
    pages[index].append(corner);
  }
}

$('reader').addEventListener('pointerdown', event => {
  const corner = event.target.closest('.page-corner');
  if (!corner || event.button !== 0 || !renderReady || captureMode || pageZoom.factor > 1) return;
  event.preventDefault();
  const rect = corner.closest('.pdf-page').getBoundingClientRect();
  const delta = Number(corner.dataset.delta);
  const side = corner.classList.contains('right') ? 1 : -1;
  const gesture = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, lastX: event.clientX, lastTime: performance.now(), width: rect.width, side, delta, cornerY: (event.clientY - rect.top) / rect.height, progress: 0, vertical: 0, velocity: 0, excursion: 0, released: false, commit: false };
  $('reader').setPointerCapture(event.pointerId);
  $('reader').dataset.grabbing = 'true';
  turnGesture = gesture;
  navigate(state.page + delta * (state.mode === 'spread' ? 2 : 1), gesture);
});
$('reader').addEventListener('pointermove', event => {
  const gesture = turnGesture;
  if (!gesture || gesture.pointerId !== event.pointerId || gesture.released) return;
  event.preventDefault();
  const now = performance.now();
  gesture.progress = clamp((gesture.x - event.clientX) * gesture.side / gesture.width, 0, 1);
  gesture.vertical = event.clientY - gesture.y;
  gesture.excursion = Math.max(gesture.excursion, Math.hypot(event.clientX - gesture.x, gesture.vertical));
  gesture.velocity = (gesture.lastX - event.clientX) * gesture.side / gesture.width * 1000 / Math.max(1, now - gesture.lastTime);
  gesture.lastX = event.clientX; gesture.lastTime = now;
  gesture.session?.update(gesture.progress, gesture.vertical);
});
function releasePaper(event) {
  const gesture = turnGesture;
  if (!gesture || gesture.pointerId !== event.pointerId || gesture.released) return;
  const click = Math.max(gesture.excursion, Math.hypot(event.clientX - gesture.x, event.clientY - gesture.y)) < 5;
  gesture.velocity *= Math.max(0, 1 - (performance.now() - gesture.lastTime) / 120);
  gesture.released = true;
  gesture.commit = event.type !== 'pointercancel' && (click || gesture.progress > .28 || gesture.velocity > .8);
  if ($('reader').hasPointerCapture(event.pointerId)) $('reader').releasePointerCapture(event.pointerId);
  if (gesture.session) gesture.session.settle(gesture.commit, gesture.velocity);
  else if (!gesture.commit) {
    ++rendering;
    cancelRender();
    state.page = displayedPage;
    renderReady = true;
    busy(null); delete $('reader').dataset.loading;
    syncControls();
  }
}
$('reader').addEventListener('pointerup', releasePaper);
$('reader').addEventListener('pointercancel', releasePaper);

renderIcons();
syncControls();
renderClips();
$('open-file').addEventListener('click', () => action($('open-file'), async () => openDocument(await desktop.openPdf())));
$('open-file-empty').addEventListener('click', () => action($('open-file-empty'), async () => openDocument(await desktop.openPdf())));
$('prev-page').addEventListener('click', () => step(-1));
$('next-page').addEventListener('click', () => step(1));
$('page-input').addEventListener('change', () => navigate($('page-input').value));
$('page-input').addEventListener('keydown', event => { if (event.key === 'Enter') { navigate($('page-input').value); $('reader').focus(); } });
$('zoom-out').addEventListener('click', () => zoom(-.5));
$('zoom-in').addEventListener('click', () => zoom(.5));
$('fit-page').addEventListener('click', () => pageZoom.set(1));
$('page-map-toggle').addEventListener('click', showPageMap);
pageMap.dialog.addEventListener('close', () => $('page-map-toggle').setAttribute('aria-expanded', 'false'));
$('tools-toggle').addEventListener('click', () => {
  const show = $('reader-tools').hidden;
  closePanels();
  $('reader-tools').hidden = !show;
  $('tools-toggle').setAttribute('aria-expanded', String(show));
});
document.addEventListener('pointerdown', event => {
  if (!event.target.closest('#reader-tools, #tools-toggle, #sidebar, #clips-panel, #toggle-sidebar, #clips-toggle')) closePanels();
});
function changePreferences(change) {
  preferences = { ...preferences, ...change };
  Object.assign(state, preferences); syncControls();
  if ('mode' in change || 'direction' in change) renderPages();
  const snapshot = { ...preferences };
  preferenceSave = preferenceSave.then(() => desktop.savePreferences(snapshot)).catch(() => toast('表示設定を保存できませんでした。', true));
}
$('theme-select').addEventListener('change', event => changePreferences({ theme: event.target.value }));
$('layout-select').addEventListener('change', event => changePreferences({ mode: event.target.value }));
$('layout-single').addEventListener('click', () => changePreferences({ mode: 'single' }));
$('layout-spread').addEventListener('click', () => changePreferences({ mode: 'spread' }));
$('layout-toggle').addEventListener('click', () => changePreferences({ mode: state.mode === 'spread' ? 'single' : 'spread' }));
$('direction-select').addEventListener('change', event => changePreferences({ direction: event.target.value }));
$('focus-toggle').addEventListener('click', focusMode);
$('toggle-sidebar').addEventListener('click', () => setPanel('sidebar', $('sidebar').hidden));
$('clips-toggle').addEventListener('click', () => setPanel('clips-panel', $('clips-panel').hidden));
document.querySelectorAll('[data-sidebar-tab]').forEach(button => button.addEventListener('click', () => { sidebarTab = button.dataset.sidebarTab; renderSidebar(); }));
$('bookmark-toggle').addEventListener('click', bookmark);
$('copy-page').addEventListener('click', () => {
  const page = state.page;
  const name = book?.name;
  const currentPdf = pdf;
  action($('copy-page'), async () => {
    const text = await getText(page, currentPdf);
    if (!text) return toast('文章のないページです。「切り抜く」で画像にできます。', true);
    await desktop.copyText(citation(name, page, text));
    addClip({ text, page, name });
    toast('書名とページ番号を添えてコピーしました');
  });
});
$('copy-all-clips').addEventListener('click', () => action($('copy-all-clips'), () => desktop.copyText(clips.filter(clip => clip.text).map(clip => citation(clip.name, clip.page, clip.text)).join('\n\n---\n\n')), '文章をまとめてコピーしました'));
$('capture-region').addEventListener('click', () => setCapture(!captureMode));
$('search-toggle').addEventListener('click', () => toggleSearch());
$('search-input').addEventListener('input', () => { ++searchToken; clearTimeout(searchTimer); searchTimer = setTimeout(search, 230); });
$('search-input').addEventListener('keydown', event => { if (event.key === 'Enter') { event.preventDefault(); visitResult(event.shiftKey ? -1 : 1); } });
$('search-prev').addEventListener('click', () => visitResult(-1));
$('search-next').addEventListener('click', () => visitResult(1));
$('help-toggle').addEventListener('click', () => $('help-dialog').showModal());
$('help-close').addEventListener('click', () => $('help-dialog').close());
function cancelPassword() {
  ++opening;
  loadingTask?.destroy().catch(() => {});
  loadingTask = null;
  $('password-input').value = '';
  $('password-dialog').close();
  $('welcome').hidden = false;
  $('reader').hidden = true;
  busy(null);
  toast('PDFを開く操作をキャンセルしました');
}
$('password-cancel').addEventListener('click', cancelPassword);
$('password-dialog').addEventListener('cancel', event => { event.preventDefault(); cancelPassword(); });

selectionToolbar.addEventListener('pointerdown', event => event.preventDefault());
selectionCopy.addEventListener('click', async () => {
  if (selectedExcerpt) await copyExcerpt(selectedExcerpt.text, selectedExcerpt.page, selectionCopy, selectedExcerpt.name);
  selectionToolbar.hidden = true;
});
document.addEventListener('selectionchange', () => {
  const selection = window.getSelection();
  const anchor = selection?.anchorNode?.parentElement?.closest('.pdf-page');
  const focus = selection?.focusNode?.parentElement?.closest('.pdf-page');
  if (captureMode || !selection?.toString().trim() || !anchor || !focus) { selectionToolbar.hidden = true; selectedExcerpt = null; return; }
  const rect = selection.getRangeAt(0).getBoundingClientRect();
  selectedExcerpt = { text: selection.toString(), page: Math.min(Number(anchor.dataset.pageNumber), Number(focus.dataset.pageNumber)), name: book.name };
  selectionToolbar.hidden = false;
  selectionToolbar.style.left = `${clamp(rect.left + rect.width / 2 - 80, 8, innerWidth - 190)}px`;
  selectionToolbar.style.top = `${Math.max(64, rect.top - 44)}px`;
});
document.addEventListener('copy', event => {
  if (/INPUT|TEXTAREA/.test(document.activeElement?.tagName)) return;
  if (!selectedExcerpt || !window.getSelection()?.toString().trim()) return;
  event.preventDefault();
  event.clipboardData.setData('text/plain', citation(selectedExcerpt.name, selectedExcerpt.page, selectedExcerpt.text));
  toast('書名とページ番号を添えてコピーしました');
});

document.addEventListener('keydown', event => {
  if (event.defaultPrevented) return;
  if (document.querySelector('dialog[open]')) return;
  const typing = /INPUT|TEXTAREA|SELECT/.test(event.target.tagName);
  const command = event.metaKey || event.ctrlKey;
  if (event.key === 'Escape') {
    if (turnGesture && !turnGesture.released) {
      event.preventDefault();
      releasePaper({ type: 'pointercancel', pointerId: turnGesture.pointerId, clientX: turnGesture.lastX, clientY: turnGesture.y + turnGesture.vertical });
      return;
    }
    if (!$('reader-tools').hidden || !$('sidebar').hidden || !$('clips-panel').hidden) { closePanels(); return; }
    if (captureMode) { setCapture(false); toast('切り抜きをキャンセルしました'); }
    else if (!$('search-panel').hidden) toggleSearch(false);
    else if (pageZoom.factor > 1) pageZoom.set(1);
    else if (document.body.dataset.focus === 'true') focusMode();
    return;
  }
  if (command && event.key.toLowerCase() === 'o') { event.preventDefault(); $('open-file').click(); return; }
  if (command && event.key.toLowerCase() === 'f') { event.preventDefault(); event.shiftKey ? focusMode() : toggleSearch(true); return; }
  if (command && event.shiftKey && event.key.toLowerCase() === 'x') { event.preventDefault(); if (pdf) setCapture(!captureMode); return; }
  if (command && event.key.toLowerCase() === 'd') { event.preventDefault(); bookmark(); return; }
  if (command && ['+', '=', '-', '0'].includes(event.key)) {
    event.preventDefault();
    if (event.key === '0') pageZoom.set(1); else zoom(event.key === '-' ? -.5 : .5);
    return;
  }
  if ((!typing || command) && event.key.toLowerCase() === 'g') { event.preventDefault(); showPageMap(); return; }
  if (typing || command) return;
  if (event.key === ' ') {
    if (event.target.closest('button, a')) return;
    event.preventDefault(); step(event.shiftKey ? -1 : 1); return;
  }
  if (['ArrowRight', 'ArrowLeft', 'PageDown', 'PageUp'].includes(event.key)) {
    event.preventDefault();
    let delta = ['ArrowRight', 'PageDown'].includes(event.key) ? 1 : -1;
    if (state.direction === 'rtl' && event.key.startsWith('Arrow')) delta *= -1;
    step(delta);
  } else if (event.key === 'Home') { event.preventDefault(); navigate(1); }
  else if (event.key === 'End') { event.preventDefault(); navigate(pdf?.numPages || 1); }
  else if (event.key === '?' || event.key === 'h') $('help-dialog').showModal();
});

if (desktop?.native !== 'tauri') {
let dragDepth = 0;
document.addEventListener('dragenter', event => { if (event.dataTransfer.types.includes('Files')) { event.preventDefault(); dragDepth++; $('drop-overlay').hidden = false; } });
document.addEventListener('dragover', event => { if (event.dataTransfer.types.includes('Files')) { event.preventDefault(); event.dataTransfer.dropEffect = 'copy'; } });
document.addEventListener('dragleave', event => { event.preventDefault(); if (--dragDepth <= 0) { dragDepth = 0; $('drop-overlay').hidden = true; } });
document.addEventListener('drop', event => {
  event.preventDefault(); dragDepth = 0; $('drop-overlay').hidden = true;
  const file = [...event.dataTransfer.files].find(file => /\.pdf$/i.test(file.name));
  if (!file) return toast('PDFファイルをドロップしてください', true);
  action($('open-file'), async () => openDocument(await desktop.openDroppedPdf(file)));
});

}

let resizeTimer;
let lastSize = '';
new ResizeObserver(entries => {
  const rect = entries[0].contentRect;
  const size = `${Math.round(rect.width)}:${Math.round(rect.height)}`;
  if (size === lastSize) return;
  lastSize = size;
  clearTimeout(resizeTimer);
  if (pdf) resizeTimer = setTimeout(renderPages, 110);
}).observe($('reader'));

if (desktop) {
  desktop.onOpenPdf(openDocument);
  desktop.onOpenError(message => { busy(null); toast(message, true); });
  desktop.onCommand(command => {
    if (command === 'open') $('open-file').click();
    else if (command === 'find') toggleSearch(true);
    else if (command === 'focus') focusMode();
    else if (command === 'capture') setCapture(!captureMode);
  });
  desktop.onFileDrag?.(dragging => { $('drop-overlay').hidden = !dragging; });
  (async () => { await desktop.ready?.(); return refreshLibrary(); })().then(async data => {
    if (data.lastOpenedId && !opening && !desktop.hasPendingOpen) {
      try {
        const last = await desktop.reopenPdf(data.lastOpenedId);
        if (!opening) await openDocument(last);
      } catch { toast('前回のPDFが見つかりません。「開く」から選び直してください。', true); }
    }
  }).catch(() => toast('本棚を読み込めませんでした。PDFは「開く」から読めます。', true));
} else {
  showError('Sumiアプリから開いてください。');
}
