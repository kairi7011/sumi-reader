import * as pdfjs from '../node_modules/pdfjs-dist/build/pdf.mjs';
import { renderIcons, icon } from './icons.js';
import { clamp, normalizeState, visiblePages, cropRect, pageText, textSegments, matchingSegments, citation, searchMatches } from './model.js';

pdfjs.GlobalWorkerOptions.workerSrc = new URL('../node_modules/pdfjs-dist/build/pdf.worker.mjs', import.meta.url).href;
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
let renderTasks = [];
let pageProxies = [];
let textLayers = [];
let toastTimer;
let searchTimer;
let searchToken = 0;
let searchResults = [];
let resultIndex = -1;
let captureMode = false;
let renderReady = false;
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
  const snapshot = structuredClone(state);
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
  $('zoom-value').textContent = `${Math.round(state.zoom * 100)}%`;
  $('zoom-out').disabled = !pdf || state.zoom <= .35;
  $('zoom-in').disabled = !pdf || state.zoom >= 3;
  $('capture-region').disabled = !pdf || !renderReady;
  $('fit-width').setAttribute('aria-pressed', String(state.fit === 'width'));
  $('fit-page').setAttribute('aria-pressed', String(state.fit === 'page'));
  const marked = state.bookmarks.includes(state.page);
  $('bookmark-toggle').setAttribute('aria-pressed', String(marked));
  $('bookmark-toggle').setAttribute('aria-label', marked ? 'しおりを外す' : 'しおりを付ける');
  $('layout-select').value = state.mode;
  $('direction-select').value = state.direction;
  $('theme-select').value = state.theme;
  document.body.dataset.theme = state.theme;
  document.body.dataset.direction = state.direction;
  $('pages').dataset.direction = state.direction;
  $('footer-status').textContent = pdf ? '読書位置を自動保存' : 'PDFを開いて読み始める';
  $('document-title').textContent = book?.name || 'Sumi';
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
      name.textContent = item.name.replace(/\.pdf$/i, '');
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
  renderSidebar();
  return data;
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
  $('password-dialog').close();
  $('password-form').onsubmit = null;
  $('password-input').value = '';
  ++searchToken;
  ++rendering;
  renderReady = false;
  setCapture(false);
  selectionToolbar.hidden = true;
  if (book && pdf) await desktop.saveReadingState(book.id, state).catch(() => {});
  if (token !== opening) return;
  cancelRender();
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
    book = { id: document.id, name: document.name };
    state = normalizeState(document.state, pdf.numPages);
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

function cancelRender() {
  renderTasks.forEach(task => task.cancel());
  textLayers.forEach(layer => layer.cancel());
  renderTasks = [];
  textLayers = [];
  for (const page of pageProxies) page.cleanup();
  pageProxies = [];
}

async function renderPages() {
  if (!pdf) return;
  clearTimeout(resizeTimer);
  const token = ++rendering;
  renderReady = false;
  $('capture-region').disabled = true;
  const currentPdf = pdf;
  cancelRender();
  setCapture(false);
  selectionToolbar.hidden = true;
  $('reader-status').hidden = true;
  busy('ページを描画しています…');
  $('pages').replaceChildren();
  const numbers = visiblePages(state.page, pdf.numPages, state.mode);
  try {
    const proxies = await Promise.all(numbers.map(page => currentPdf.getPage(page)));
    if (token !== rendering) return;
    pageProxies = proxies;
    const base = proxies.map(page => page.getViewport({ scale: 1 }));
    const reader = $('reader');
    const style = getComputedStyle(reader);
    const availableWidth = reader.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - (numbers.length - 1) * 18;
    const availableHeight = reader.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom);
    const fitWidth = availableWidth / base.reduce((sum, page) => sum + page.width, 0);
    const fitHeight = availableHeight / Math.max(...base.map(page => page.height));
    if (state.fit !== 'custom') state.zoom = clamp(state.fit === 'width' ? fitWidth : Math.min(fitWidth, fitHeight), .1, 3);
    syncControls();
    $('capture-region').disabled = true;
    await Promise.all(proxies.map(async (page, index) => {
      const viewport = page.getViewport({ scale: state.zoom });
      const wrapper = document.createElement('div');
      wrapper.className = 'pdf-page';
      wrapper.dataset.pageNumber = numbers[index];
      wrapper.setAttribute('aria-label', `${numbers[index]} ページ`);
      wrapper.style.width = `${viewport.width}px`;
      wrapper.style.height = `${viewport.height}px`;
      wrapper.style.setProperty('--total-scale-factor', viewport.scale);
      const canvas = document.createElement('canvas');
      // Large zooms must not allocate unbounded Retina backing stores.
      const outputScale = Math.min(devicePixelRatio || 1, 2, Math.sqrt(14_000_000 / (viewport.width * viewport.height)));
      canvas.width = Math.floor(viewport.width * outputScale);
      canvas.height = Math.floor(viewport.height * outputScale);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      const layerDiv = document.createElement('div');
      layerDiv.className = 'textLayer';
      wrapper.append(canvas, layerDiv);
      $('pages').append(wrapper);
      const renderTask = page.render({ canvasContext: canvas.getContext('2d', { alpha: false }), viewport, transform: [outputScale, 0, 0, outputScale, 0, 0] });
      renderTasks.push(renderTask);
      await renderTask.promise;
      if (token !== rendering) return;
      const content = await page.getTextContent();
      if (token !== rendering) return;
      cacheText(numbers[index], pageText(content));
      const textLayer = new pdfjs.TextLayer({ textContentSource: content, container: layerDiv, viewport });
      textLayer.sumiSegments = textSegments(content);
      textLayers.push(textLayer);
      await textLayer.render();
    }));
    if (token !== rendering) return;
    renderReady = true;
    $('capture-region').disabled = false;
    reader.scrollTop = 0;
    reader.scrollLeft = state.direction === 'rtl' ? reader.scrollWidth : 0;
    highlightSearch();
    persist();
  } catch (error) {
    if (token !== rendering || error.name === 'RenderingCancelledException' || error.name === 'AbortException') return;
    console.error('Page render failed', error);
    showError('このページを表示できませんでした。ページを移動するか、PDFを開き直してください。');
  } finally {
    if (token === rendering) busy(null);
  }
}

async function navigate(page) {
  if (!pdf) return;
  const next = clamp(Math.floor(Number(page)) || 1, 1, pdf.numPages);
  if (next === state.page) return;
  state.page = next;
  syncControls();
  renderSidebar();
  persist();
  await renderPages();
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
  const text = pageText(await proxy.getTextContent());
  if (currentPdf === pdf) cacheText(page, text);
  if (!pageProxies.includes(proxy)) proxy.cleanup();
  return text;
}

function setPanel(id, show) {
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
    const point = event => { const bounds = overlay.getBoundingClientRect(); return { x: event.clientX - bounds.left, y: event.clientY - bounds.top }; };
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
  if (!pdf) return;
  const exists = state.bookmarks.includes(state.page);
  state.bookmarks = exists ? state.bookmarks.filter(page => page !== state.page) : [...state.bookmarks, state.page].sort((a, b) => a - b);
  syncControls(); renderSidebar(); persist();
  toast(exists ? 'しおりを外しました' : `${state.page}ページにしおりを付けました`);
}

function focusMode() {
  const focused = document.body.dataset.focus !== 'true';
  document.body.dataset.focus = String(focused);
  $('focus-toggle').setAttribute('aria-pressed', String(focused));
  toast(focused ? '集中モード · Escで戻る' : '通常表示に戻りました');
}

function zoom(delta) {
  if (!pdf) return;
  state.zoom = clamp(Math.round((state.zoom + delta) * 100) / 100, .35, 3);
  state.fit = 'custom';
  renderPages();
}

renderIcons();
syncControls();
renderClips();
$('open-file').addEventListener('click', () => action($('open-file'), async () => openDocument(await desktop.openPdf())));
$('open-file-empty').addEventListener('click', () => action($('open-file-empty'), async () => openDocument(await desktop.openPdf())));
$('prev-page').addEventListener('click', () => step(-1));
$('next-page').addEventListener('click', () => step(1));
$('page-input').addEventListener('change', () => navigate($('page-input').value));
$('page-input').addEventListener('keydown', event => { if (event.key === 'Enter') { navigate($('page-input').value); $('reader').focus(); } });
$('zoom-out').addEventListener('click', () => zoom(-.15));
$('zoom-in').addEventListener('click', () => zoom(.15));
for (const fit of ['width', 'page']) $(`fit-${fit}`).addEventListener('click', () => { state.fit = fit; renderPages(); });
$('theme-select').addEventListener('change', event => { state.theme = event.target.value; syncControls(); persist(); });
$('layout-select').addEventListener('change', event => { state.mode = event.target.value; syncControls(); renderPages(); persist(); });
$('direction-select').addEventListener('change', event => { state.direction = event.target.value; syncControls(); renderPages(); persist(); });
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
  const typing = /INPUT|TEXTAREA|SELECT/.test(event.target.tagName);
  const command = event.metaKey || event.ctrlKey;
  if (event.key === 'Escape') {
    if (captureMode) { setCapture(false); toast('切り抜きをキャンセルしました'); }
    else if (!$('search-panel').hidden) toggleSearch(false);
    else if (document.body.dataset.focus === 'true') focusMode();
    return;
  }
  if (command && event.key.toLowerCase() === 'o') { event.preventDefault(); $('open-file').click(); return; }
  if (command && event.key.toLowerCase() === 'f') { event.preventDefault(); event.shiftKey ? focusMode() : toggleSearch(true); return; }
  if (command && event.shiftKey && event.key.toLowerCase() === 'x') { event.preventDefault(); if (pdf) setCapture(!captureMode); return; }
  if (command && event.key.toLowerCase() === 'd') { event.preventDefault(); bookmark(); return; }
  if (command && ['+', '=', '-', '0'].includes(event.key)) {
    event.preventDefault();
    if (event.key === '0') { state.fit = 'page'; renderPages(); } else zoom(event.key === '-' ? -.15 : .15);
    return;
  }
  if (typing || command || document.querySelector('dialog[open]')) return;
  if (['ArrowRight', 'ArrowLeft', 'PageDown', 'PageUp'].includes(event.key)) {
    event.preventDefault();
    let delta = ['ArrowRight', 'PageDown'].includes(event.key) ? 1 : -1;
    if (state.direction === 'rtl' && event.key.startsWith('Arrow')) delta *= -1;
    step(delta);
  } else if (event.key === 'Home') { event.preventDefault(); navigate(1); }
  else if (event.key === 'End') { event.preventDefault(); navigate(pdf?.numPages || 1); }
  else if (event.key === '?' || event.key === 'h') $('help-dialog').showModal();
});

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

let resizeTimer;
let lastSize = '';
new ResizeObserver(entries => {
  const rect = entries[0].contentRect;
  const size = `${Math.round(rect.width)}:${Math.round(rect.height)}`;
  if (size === lastSize) return;
  lastSize = size;
  clearTimeout(resizeTimer);
  if (pdf && state.fit !== 'custom') resizeTimer = setTimeout(renderPages, 110);
}).observe($('reader'));

if (desktop) {
  desktop.onOpenPdf(openDocument);
  desktop.onOpenError(message => { busy(null); toast(message, true); });
  desktop.onCommand(command => {
    if (command === 'find') toggleSearch(true);
    else if (command === 'focus') focusMode();
    else if (command === 'capture') setCapture(!captureMode);
  });
  refreshLibrary().then(async data => {
    if (data.lastOpenedId && !opening) {
      try {
        const last = await desktop.reopenPdf(data.lastOpenedId);
        if (!opening) await openDocument(last);
      } catch { toast('前回のPDFが見つかりません。「開く」から選び直してください。', true); }
    }
  }).catch(() => toast('本棚を読み込めませんでした。PDFは「開く」から読めます。', true));
} else {
  showError('Sumiアプリから開いてください。');
}
