import { clamp } from './model.js';

export class PageZoom {
  constructor({ reader, getDocument, onChange, notify }) {
    Object.assign(this, { reader, getDocument, onChange, notify });
    this.factor = 1;
    this.x = this.y = this.token = 0;
    this.tasks = [];
    reader.addEventListener('wheel', event => {
      if (event.ctrlKey) {
        event.preventDefault();
        this.set(this.factor * Math.exp(-event.deltaY * .01), event);
      } else if (this.factor > 1) {
        event.preventDefault();
        this.pan(this.x - event.deltaX, this.y - event.deltaY);
      }
    }, { passive: false });
    reader.addEventListener('gesturestart', event => { event.preventDefault(); this.gestureScale = this.factor; }, { passive: false });
    reader.addEventListener('gesturechange', event => {
      event.preventDefault(); this.set(this.gestureScale * event.scale, event);
    }, { passive: false });
    reader.addEventListener('gestureend', event => event.preventDefault(), { passive: false });
    document.addEventListener('keydown', event => {
      if (event.key !== ' ' || this.factor === 1 || event.metaKey || event.ctrlKey || event.target.closest('button, input, select, textarea, dialog')) return;
      event.preventDefault(); this.space = true; reader.dataset.panReady = 'true';
    });
    document.addEventListener('keyup', event => { if (event.key === ' ') this.release(); });
    window.addEventListener('blur', () => this.release());
    reader.addEventListener('pointerdown', event => {
      if (!this.space || this.factor === 1 || event.button !== 0) return;
      event.preventDefault(); event.stopImmediatePropagation();
      reader.setPointerCapture(event.pointerId);
      this.drag = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, startX: this.x, startY: this.y };
      reader.dataset.panning = 'true';
    }, true);
    reader.addEventListener('pointermove', event => {
      if (!this.drag) return;
      event.preventDefault();
      this.pan(this.drag.startX + event.clientX - this.drag.x, this.drag.startY + event.clientY - this.drag.y);
    });
    reader.addEventListener('pointerup', () => this.endDrag());
    reader.addEventListener('pointercancel', () => this.endDrag());
  }

  endDrag() {
    if (this.drag && this.reader.hasPointerCapture(this.drag.pointerId)) this.reader.releasePointerCapture(this.drag.pointerId);
    this.drag = null;
    delete this.reader.dataset.panning;
  }

  release() {
    this.endDrag(); this.space = false;
    delete this.reader.dataset.panReady;
  }

  geometry() {
    const node = document.getElementById('pages');
    if (!node?.querySelector('.pdf-page') || this.reader.dataset.loading === 'true') return null;
    const transform = node.style.transform;
    node.style.transform = '';
    const base = node.getBoundingClientRect();
    const pages = [...node.querySelectorAll('.pdf-page')].map(page => page.getBoundingClientRect());
    node.style.transform = transform;
    const rect = this.reader.getBoundingClientRect(), style = getComputedStyle(this.reader);
    return { node, base,
      paper: { left: Math.min(...pages.map(p => p.left)), right: Math.max(...pages.map(p => p.right)), top: Math.min(...pages.map(p => p.top)), bottom: Math.max(...pages.map(p => p.bottom)) },
      view: { left: rect.left + parseFloat(style.paddingLeft), right: rect.right - parseFloat(style.paddingRight), top: rect.top + parseFloat(style.paddingTop), bottom: rect.bottom - parseFloat(style.paddingBottom) },
    };
  }

  set(value, point) {
    if (!Number.isFinite(value)) return;
    const geometry = this.geometry();
    if (!geometry) return;
    const next = clamp(value, 1, 4);
    const { base, view } = geometry;
    const x = Number.isFinite(point?.clientX) ? point.clientX : (view.left + view.right) / 2;
    const y = Number.isFinite(point?.clientY) ? point.clientY : (view.top + view.bottom) / 2;
    this.x = x - base.left - (x - base.left - this.x) * next / this.factor;
    this.y = y - base.top - (y - base.top - this.y) * next / this.factor;
    this.factor = next;
    this.apply(geometry);
    this.schedule(); this.onChange();
  }

  zoom(delta) {
    const levels = [1, 1.25, 1.5, 2, 3, 4];
    this.set(delta > 0 ? levels.find(value => value > this.factor + .01) || 4 : levels.findLast(value => value < this.factor - .01) || 1);
  }

  pan(x, y) {
    const geometry = this.geometry();
    if (!geometry) return;
    this.x = x; this.y = y; this.apply(geometry);
  }

  apply({ node, base, paper, view }) {
    const bound = (value, start, end, near, far, origin) => {
      const low = far - origin - (end - origin) * this.factor;
      const high = near - origin - (start - origin) * this.factor;
      return low > high ? (low + high) / 2 : clamp(value, low, high);
    };
    this.x = bound(this.x, paper.left, paper.right, view.left, view.right, base.left);
    this.y = bound(this.y, paper.top, paper.bottom, view.top, view.bottom, base.top);
    if (this.factor === 1) this.x = this.y = 0;
    node.style.transformOrigin = '0 0';
    node.style.transform = this.factor === 1 ? '' : `translate(${this.x}px, ${this.y}px) scale(${this.factor})`;
    this.reader.dataset.zoomed = String(this.factor > 1);
  }

  cancel() {
    ++this.token; clearTimeout(this.timer);
    this.tasks.forEach(task => task.cancel()); this.tasks = [];
    delete this.reader.dataset.zoomRendering;
  }

  reset() {
    this.cancel(); this.release();
    this.factor = 1; this.x = this.y = 0;
    const node = document.getElementById('pages');
    if (node) node.style.transform = '';
    delete this.reader.dataset.zoomed;
    this.onChange();
  }

  schedule() {
    this.cancel();
    const token = this.token, factor = this.factor, pdf = this.getDocument();
    const wrappers = [...document.querySelectorAll('#pages .pdf-page')];
    this.reader.dataset.zoomRendering = 'true';
    this.timer = setTimeout(async () => {
      try {
        for (const wrapper of wrappers) {
          const page = await pdf.getPage(Number(wrapper.dataset.pageNumber));
          if (token !== this.token) return;
          const base = page.getViewport({ scale: 1 });
          const width = parseFloat(wrapper.style.width) || wrapper.clientWidth;
          const height = parseFloat(wrapper.style.height) || wrapper.clientHeight;
          const viewport = page.getViewport({ scale: width / base.width * factor });
          const ratio = Math.min(devicePixelRatio || 1, 2, Math.sqrt(14_000_000 / (viewport.width * viewport.height)));
          const canvas = document.createElement('canvas');
          canvas.width = Math.ceil(viewport.width * ratio); canvas.height = Math.ceil(viewport.height * ratio);
          canvas.style.width = `${width}px`; canvas.style.height = `${height}px`;
          const task = page.render({ canvasContext: canvas.getContext('2d', { alpha: false }), viewport, transform: [ratio, 0, 0, ratio, 0, 0] });
          this.tasks.push(task);
          try {
            await task.promise;
            if (token !== this.token || !wrapper.isConnected) { canvas.width = canvas.height = 0; return; }
            const previous = wrapper.querySelector('canvas');
            previous.replaceWith(canvas); previous.width = previous.height = 0;
          } catch (error) { canvas.width = canvas.height = 0; throw error; }
        }
      } catch (error) {
        if (token === this.token && !['RenderingCancelledException', 'AbortException'].includes(error.name)) this.notify('高解像度の表示を準備できませんでした。もう一度拡大してください。', true);
      } finally { if (token === this.token) { this.tasks = []; delete this.reader.dataset.zoomRendering; } }
    }, 160);
  }
}
