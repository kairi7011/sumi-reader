import { icon } from './icons.js';
import { clamp } from './model.js';

export class Magnifier {
  constructor({ getDocument, copyImage, notify, onChange = () => {} }) {
    Object.assign(this, { getDocument, copyImage, notify, onChange });
    this.factor = 2;
    this.token = 0;
    this.panel = document.createElement('section');
    this.panel.id = 'magnifier';
    this.panel.hidden = true;
    this.panel.setAttribute('role', 'dialog');
    this.panel.setAttribute('aria-label', '部分拡大');
    this.panel.innerHTML = `<header><strong>部分拡大</strong><span id="magnifier-page"></span><button id="magnifier-minus" aria-label="拡大率を下げる">${icon('minus')}</button><output id="magnifier-scale"></output><button id="magnifier-plus" aria-label="拡大率を上げる">${icon('plus')}</button><button id="magnifier-close" aria-label="部分拡大を閉じる">${icon('close')}</button></header><div class="magnifier-image"><canvas id="magnifier-canvas"></canvas><span id="magnifier-busy" hidden>鮮明にしています…</span></div><footer><span>ドラッグで見る場所を移動</span><button id="magnifier-copy">${icon('copy')}画像をコピー</button></footer>`;
    document.body.append(this.panel);
    this.canvas = this.panel.querySelector('canvas');
    this.panel.querySelector('#magnifier-close').onclick = () => this.close();
    this.panel.querySelector('#magnifier-minus').onclick = () => this.zoom(-.5);
    this.panel.querySelector('#magnifier-plus').onclick = () => this.zoom(.5);
    this.panel.querySelector('#magnifier-copy').onclick = () => this.copy();
    this.canvas.addEventListener('pointerdown', event => {
      if (event.button !== 0) return;
      event.preventDefault();
      this.canvas.setPointerCapture(event.pointerId);
      this.drag = { x: event.clientX, y: event.clientY, cx: this.center.x, cy: this.center.y };
      this.canvas.dataset.dragging = 'true';
    });
    this.canvas.addEventListener('pointermove', event => {
      if (!this.drag) return;
      this.center.x = this.drag.cx - (event.clientX - this.drag.x) / this.factor;
      this.center.y = this.drag.cy - (event.clientY - this.drag.y) / this.factor;
      this.draw();
    });
    const release = () => { this.drag = null; delete this.canvas.dataset.dragging; };
    this.canvas.addEventListener('pointerup', release);
    this.canvas.addEventListener('pointercancel', release);
    this.canvas.addEventListener('keydown', event => {
      if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
      event.preventDefault(); event.stopPropagation();
      this.center.x += ({ ArrowLeft: -35, ArrowRight: 35 }[event.key] || 0) / this.factor;
      this.center.y += ({ ArrowUp: -35, ArrowDown: 35 }[event.key] || 0) / this.factor;
      this.draw();
    });
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute('aria-label', '拡大部分。ドラッグまたは矢印キーで見る場所を移動');
  }

  get isOpen() { return !this.panel.hidden; }

  open(wrapper, point) {
    if (!wrapper?.querySelector('canvas')?.width) return;
    this.returnFocus = this.isOpen ? this.returnFocus : document.activeElement;
    this.source = wrapper;
    this.panel.hidden = false;
    const rect = wrapper.getBoundingClientRect();
    this.center = { x: point ? point.x - rect.left : rect.width / 2, y: point ? point.y - rect.top : rect.height / 2 };
    this.marker?.remove();
    this.marker = document.createElement('div');
    this.marker.className = 'magnifier-source';
    wrapper.append(this.marker);
    this.draw();
    this.canvas.focus({ preventScroll: true });
    this.onChange();
  }

  close() {
    ++this.token;
    clearTimeout(this.timer);
    clearTimeout(this.busyTimer);
    this.task?.cancel(); this.task = null;
    this.panel.hidden = true;
    this.marker?.remove(); this.marker = null;
    this.source = null; this.drag = null;
    this.canvas.width = this.canvas.height = 0;
    this.detail?.remove(); this.detail = null;
    this.returnFocus?.isConnected && this.returnFocus.focus({ preventScroll: true });
    this.returnFocus = null;
    this.onChange();
  }

  zoom(delta) {
    this.factor = clamp(this.factor + delta, 1.5, 5);
    if (this.isOpen) this.draw();
    this.onChange();
  }

  draw() {
    const token = ++this.token;
    clearTimeout(this.timer);
    clearTimeout(this.busyTimer);
    this.task?.cancel(); this.task = null;
    const source = this.source.querySelector('canvas');
    const rect = this.source.getBoundingClientRect();
    const view = this.panel.querySelector('.magnifier-image');
    const width = view.clientWidth, height = view.clientHeight;
    const ratio = Math.min(devicePixelRatio || 1, 2);
    const cropWidth = Math.min(rect.width, width / this.factor);
    const cropHeight = Math.min(rect.height, height / this.factor);
    this.center.x = clamp(this.center.x, cropWidth / 2, rect.width - cropWidth / 2);
    this.center.y = clamp(this.center.y, cropHeight / 2, rect.height - cropHeight / 2);
    const left = this.center.x - width / this.factor / 2;
    const top = this.center.y - height / this.factor / 2;
    this.canvas.width = Math.round(width * ratio);
    this.canvas.height = Math.round(height * ratio);
    const context = this.canvas.getContext('2d', { alpha: false });
    context.fillStyle = '#fff'; context.fillRect(0, 0, this.canvas.width, this.canvas.height);
    context.drawImage(source, 0, 0, source.width, source.height, -left * this.factor * ratio, -top * this.factor * ratio, rect.width * this.factor * ratio, rect.height * this.factor * ratio);
    this.marker.style.cssText = `left:${Math.max(0, left)}px;top:${Math.max(0, top)}px;width:${cropWidth}px;height:${cropHeight}px`;
    this.panel.querySelector('#magnifier-scale').textContent = `${this.factor}×`;
    this.panel.querySelector('#magnifier-page').textContent = `p. ${this.source.dataset.pageNumber}`;
    this.panel.querySelector('#magnifier-minus').disabled = this.factor <= 1.5;
    this.panel.querySelector('#magnifier-plus').disabled = this.factor >= 5;
    this.panel.querySelector('#magnifier-copy').disabled = true;
    this.panel.querySelector('#magnifier-busy').hidden = true;
    const currentPdf = this.getDocument();
    const pageNumber = Number(this.source.dataset.pageNumber);
    this.busyTimer = setTimeout(() => { if (token === this.token) this.panel.querySelector('#magnifier-busy').hidden = false; }, 280);
    this.timer = setTimeout(async () => {
      let canvas;
      try {
        const page = await currentPdf.getPage(pageNumber);
        if (token !== this.token) return;
        const base = page.getViewport({ scale: 1 });
        const scale = rect.width / base.width * this.factor;
        const viewport = page.getViewport({ scale });
        canvas = document.createElement('canvas');
        canvas.width = this.canvas.width; canvas.height = this.canvas.height;
        this.task = page.render({ canvasContext: canvas.getContext('2d', { alpha: false }), viewport, transform: [ratio, 0, 0, ratio, -left * this.factor * ratio, -top * this.factor * ratio] });
        await this.task.promise;
        if (token !== this.token) return;
        context.drawImage(canvas, 0, 0);
        this.panel.querySelector('#magnifier-copy').disabled = false;
      } catch (error) {
        if (token === this.token && !['RenderingCancelledException', 'AbortException'].includes(error.name)) {
          this.panel.querySelector('#magnifier-copy').disabled = false;
          this.notify('鮮明な拡大に失敗しました。表示中の画像はコピーできます。', true);
        }
      } finally {
        if (canvas) canvas.width = canvas.height = 0;
        if (token === this.token) { this.task = null; clearTimeout(this.busyTimer); this.panel.querySelector('#magnifier-busy').hidden = true; }
      }
    }, 100);
  }

  async copy() {
    const button = this.panel.querySelector('#magnifier-copy');
    if (button.disabled || !this.canvas.width) return;
    button.disabled = true; button.dataset.busy = 'true';
    try { await this.copyImage(this.canvas.toDataURL('image/png')); this.notify('拡大した部分を画像でコピーしました'); }
    catch { this.notify('画像をコピーできませんでした。もう一度お試しください。', true); }
    finally { button.disabled = false; button.dataset.busy = 'false'; }
  }
}
