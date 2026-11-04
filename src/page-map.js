import { icon } from './icons.js';

const ROW_HEIGHT = 196;
const CACHE_LIMIT = 36;
const MAX_JOBS = 2;

export function pageWindow(total, width, height, scrollTop) {
  const columns = Math.max(1, Math.floor(width / 132));
  const rows = Math.ceil(total / columns);
  const firstRow = Math.max(0, Math.min(rows - 1, Math.floor(scrollTop / ROW_HEIGHT) - 1));
  const lastRow = Math.min(rows, Math.ceil((scrollTop + height) / ROW_HEIGHT) + 1);
  return { columns, height: rows * ROW_HEIGHT, first: firstRow * columns + 1, last: Math.min(total, lastRow * columns) };
}

export class PageMap {
  constructor({ getDocument, getPage, onNavigate }) {
    this.getDocument = getDocument;
    this.getPage = getPage;
    this.onNavigate = onNavigate;
    this.document = null;
    this.generation = 0;
    this.target = 1;
    this.lastOrigin = null;
    this.cells = new Map();
    this.cache = new Map();
    this.jobs = new Map();
    this.visible = new Set();
    this.failed = new Set();
    this.frame = 0;
    this.dialog = document.createElement('dialog');
    this.dialog.id = 'page-map';
    this.dialog.setAttribute('aria-labelledby', 'page-map-title');
    this.dialog.innerHTML = `
      <header class="page-map-header">
        <div><h2 id="page-map-title">ページを選ぶ</h2><p id="page-map-position"></p></div>
        <div class="page-map-header-actions"><button id="page-map-back" class="text-button" hidden></button><button id="page-map-close" class="icon-button" aria-label="ページ一覧を閉じる">${icon('close')}</button></div>
      </header>
      <div class="page-map-body">
        <section class="page-map-inspect" aria-label="移動先のプレビュー">
          <form id="page-map-form"><label for="page-map-input">ページ番号</label><div class="page-map-number"><input id="page-map-input" type="number" min="1" step="1" inputmode="numeric" autocomplete="off" /><span id="page-map-total"></span></div><button id="page-map-go" type="submit">このページへ <kbd>↵</kbd></button></form>
          <div id="page-map-preview"><canvas aria-hidden="true"></canvas><span id="page-map-preview-status" role="status"></span></div>
          <label class="sr-only" for="page-map-range">移動先のページ</label><input id="page-map-range" type="range" min="1" step="1" />
          <div class="page-map-range-labels"><span>1</span><span id="page-map-range-end"></span></div>
          <button id="page-map-current" class="text-button">現在のページを表示</button>
        </section>
        <div class="page-map-browser"><div class="page-map-grid-caption"><span>一覧から選ぶ</span><span>↑ ↓ ← → で移動 · Enter で開く</span></div><div id="page-map-scroll" tabindex="0" aria-label="ページの一覧"><div id="page-map-grid" role="group" aria-label="各ページのプレビュー"></div></div></div>
      </div>`;
    document.body.append(this.dialog);
    this.$ = id => this.dialog.querySelector(`#page-map-${id}`);
    this.scroller = this.$('scroll');
    this.grid = this.$('grid');
    this.preview = this.$('preview').querySelector('canvas');
    this.observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        const page = Number(entry.target.dataset.page);
        if (this.cells.get(page) !== entry.target) continue;
        if (entry.isIntersecting) this.visible.add(page);
        else this.visible.delete(page);
      }
      this.pump();
    }, { root: this.scroller, rootMargin: '120px 0px' });
    this.resizeObserver = new ResizeObserver(() => { if (this.dialog.open) this.layout(true); });
    this.resizeObserver.observe(this.scroller);
    this.scroller.addEventListener('scroll', () => {
      if (!this.frame) this.frame = requestAnimationFrame(() => { this.frame = 0; this.layout(); });
    });
    this.$('close').addEventListener('click', () => this.close());
    this.dialog.addEventListener('cancel', event => { event.preventDefault(); this.close(); });
    this.dialog.addEventListener('click', event => {
      if (event.target !== this.dialog) return;
      const rect = this.dialog.getBoundingClientRect();
      if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) this.close();
    });
    this.$('form').addEventListener('submit', event => { event.preventDefault(); if (this.$('input').reportValidity()) this.commit(); });
    this.$('input').addEventListener('input', () => {
      const input = this.$('input');
      this.$('go').disabled = !input.validity.valid || !input.value;
      if (!this.$('go').disabled) this.setTarget(Number(input.value), { scroll: true, keepInput: true });
    });
    this.$('range').addEventListener('input', event => this.setTarget(Number(event.target.value), { scroll: true }));
    this.$('current').addEventListener('click', () => this.setTarget(this.getPage(), { scroll: true }));
    this.$('back').addEventListener('click', () => this.commit(this.lastOrigin));
    this.dialog.addEventListener('keydown', event => this.keydown(event));
  }

  open() {
    const pdf = this.getDocument();
    if (!pdf) return;
    if (pdf !== this.document) { this.reset(); this.document = pdf; }
    if (this.dialog.open) { this.$('input').focus(); this.$('input').select(); return; }
    this.origin = this.getPage();
    this.failed.clear();
    this.$('position').textContent = `いま読んでいるページ ${this.origin} / ${pdf.numPages}`;
    this.$('total').textContent = `/ ${pdf.numPages}`;
    this.$('input').max = this.$('range').max = pdf.numPages;
    this.$('range-end').textContent = pdf.numPages;
    this.$('back').hidden = !this.lastOrigin || this.lastOrigin === this.origin;
    this.$('back').textContent = `p. ${this.lastOrigin} に戻る`;
    this.dialog.showModal();
    this.setTarget(this.origin, { scroll: true });
    this.$('input').focus();
    this.$('input').select();
  }

  close() {
    if (!this.dialog.open) return;
    this.dialog.close();
    this.generation++;
    for (const job of this.jobs.values()) job.task?.cancel();
    this.jobs.clear();
    this.preview.width = this.preview.height = 0;
  }

  reset() {
    this.close();
    this.generation++;
    this.document = null;
    this.lastOrigin = null;
    this.observer.disconnect();
    this.visible.clear();
    this.cells.clear();
    this.grid.replaceChildren();
    for (const canvas of this.cache.values()) canvas.width = canvas.height = 0;
    this.cache.clear();
    this.failed.clear();
  }

  destroy() {
    this.reset();
    cancelAnimationFrame(this.frame);
    this.resizeObserver.disconnect();
    this.dialog.remove();
  }

  setTarget(page, { scroll = false, focus = false, keepInput = false } = {}) {
    if (!this.document) return;
    this.target = Math.max(1, Math.min(this.document.numPages, Math.round(page)));
    if (!keepInput) this.$('input').value = this.target;
    this.$('range').value = this.target;
    this.$('range').setAttribute('aria-valuetext', `${this.target} ページ`);
    this.$('go').disabled = false;
    this.$('go').firstChild.textContent = `${this.target} ページへ `;
    this.layout(scroll);
    this.showPreview();
    if (focus) this.cells.get(this.target)?.focus({ preventScroll: true });
    for (const [number, job] of this.jobs) {
      if (number !== this.target && !this.cells.has(number)) job.task?.cancel();
    }
    this.pump();
  }

  layout(scroll = false) {
    if (!this.dialog.open || !this.document) return;
    const { numPages } = this.document;
    const width = this.scroller.clientWidth;
    const height = this.scroller.clientHeight;
    let layout = pageWindow(numPages, width, height, this.scroller.scrollTop);
    this.grid.style.height = `${layout.height}px`;
    if (scroll) {
      const top = Math.floor((this.target - 1) / layout.columns) * ROW_HEIGHT;
      if (top < this.scroller.scrollTop || top + ROW_HEIGHT > this.scroller.scrollTop + height) this.scroller.scrollTop = Math.max(0, top - (height - ROW_HEIGHT) / 2);
      layout = pageWindow(numPages, width, height, this.scroller.scrollTop);
    }
    this.columns = layout.columns;
    this.grid.style.height = `${layout.height}px`;
    for (const [page, cell] of this.cells) {
      if (page < layout.first || page > layout.last) {
        this.observer.unobserve(cell); this.visible.delete(page); cell.remove(); this.cells.delete(page);
      }
    }
    for (let page = layout.first; page <= layout.last; page++) {
      let cell = this.cells.get(page);
      if (!cell) {
        cell = document.createElement('button');
        cell.type = 'button';
        cell.className = 'page-map-thumb';
        cell.dataset.page = page;
        cell.setAttribute('aria-label', `${page} ページを開く`);
        const image = document.createElement('span'); image.className = 'page-map-sheet';
        const number = document.createElement('span'); number.className = 'page-map-label'; number.textContent = page;
        cell.append(image, number);
        cell.addEventListener('click', () => this.commit(page));
        cell.addEventListener('focus', () => { if (this.target !== page) this.setTarget(page); });
        this.grid.append(cell); this.cells.set(page, cell); this.observer.observe(cell);
        this.paintCell(page);
      }
      cell.style.width = `${100 / layout.columns}%`;
      cell.style.left = `${((page - 1) % layout.columns) * 100 / layout.columns}%`;
      cell.style.top = `${Math.floor((page - 1) / layout.columns) * ROW_HEIGHT}px`;
      cell.dataset.selected = String(page === this.target);
      cell.setAttribute('aria-current', page === this.origin ? 'page' : 'false');
      cell.tabIndex = page === this.target ? 0 : -1;
    }
  }

  keydown(event) {
    event.stopPropagation();
    if (event.target.matches('input, button:not(.page-map-thumb)')) return;
    const offsets = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -this.columns, ArrowDown: this.columns };
    let page = this.target;
    if (event.key in offsets) page += offsets[event.key];
    else if (event.key === 'Home') page = 1;
    else if (event.key === 'End') page = this.document.numPages;
    else if (event.key === 'Enter') { event.preventDefault(); this.commit(); return; }
    else return;
    event.preventDefault();
    this.setTarget(page, { scroll: true, focus: true });
  }

  commit(page = this.target) {
    if (!this.document || this.document !== this.getDocument()) return;
    const target = Math.max(1, Math.min(this.document.numPages, Math.round(page)));
    const origin = this.getPage();
    if (target !== origin) this.lastOrigin = origin;
    this.close();
    Promise.resolve().then(() => this.onNavigate(target)).catch(() => {});
  }

  showPreview() {
    const canvas = this.cache.get(this.target);
    const status = this.$('preview-status');
    this.$('preview').dataset.loading = String(!canvas && !this.failed.has(this.target));
    if (canvas) {
      this.touch(this.target);
      this.preview.width = canvas.width; this.preview.height = canvas.height;
      this.preview.getContext('2d').drawImage(canvas, 0, 0);
      this.preview.hidden = false; status.textContent = '';
    } else {
      this.preview.hidden = true;
      status.textContent = this.failed.has(this.target) ? 'プレビューを表示できません' : `${this.target} ページを準備しています`;
    }
  }

  paintCell(page) {
    const cell = this.cells.get(page);
    const source = this.cache.get(page);
    if (!cell || !source) return;
    const sheet = cell.querySelector('.page-map-sheet');
    if (sheet.querySelector('canvas')) return;
    const canvas = document.createElement('canvas');
    canvas.width = source.width; canvas.height = source.height;
    canvas.getContext('2d').drawImage(source, 0, 0);
    sheet.replaceChildren(canvas);
    cell.dataset.ready = 'true';
  }

  touch(page) {
    const canvas = this.cache.get(page);
    if (canvas) { this.cache.delete(page); this.cache.set(page, canvas); }
  }

  pump() {
    if (!this.dialog.open || !this.document) return;
    const candidates = [this.target, ...this.visible];
    for (const page of candidates) {
      if (this.jobs.size >= MAX_JOBS) break;
      if (this.cache.has(page) || this.jobs.has(page) || this.failed.has(page)) continue;
      this.renderThumbnail(page);
    }
  }

  async renderThumbnail(number) {
    const pdf = this.document;
    const generation = this.generation;
    const job = { task: null };
    this.jobs.set(number, job);
    let page;
    let canvas;
    try {
      page = await pdf.getPage(number);
      if (generation !== this.generation) return;
      const base = page.getViewport({ scale: 1 });
      const scale = Math.min(260 / base.width, 360 / base.height);
      const viewport = page.getViewport({ scale });
      canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.ceil(viewport.width)); canvas.height = Math.max(1, Math.ceil(viewport.height));
      job.task = page.render({ canvasContext: canvas.getContext('2d', { alpha: false }), viewport });
      await job.task.promise;
      if (generation !== this.generation) return;
      this.cache.set(number, canvas);
      this.touch(this.target);
      canvas = null;
      while (this.cache.size > CACHE_LIMIT) {
        const oldest = this.cache.keys().next().value;
        const evicted = this.cache.get(oldest); evicted.width = evicted.height = 0; this.cache.delete(oldest);
      }
      this.paintCell(number);
      if (number === this.target) this.showPreview();
    } catch (error) {
      if (generation === this.generation && error.name !== 'RenderingCancelledException' && error.name !== 'AbortException') {
        this.failed.add(number);
        if (number === this.target) this.showPreview();
      }
    } finally {
      if (canvas) canvas.width = canvas.height = 0;
      // PDF.js shares proxies with the reader; its visible spread must retain its resources.
      if (pdf === this.document && number !== this.getPage() && number !== this.getPage() + 1) { try { page?.cleanup(); } catch { /* The document may have been closed during the render. */ } }
      if (this.jobs.get(number) === job) this.jobs.delete(number);
      this.pump();
    }
  }
}
