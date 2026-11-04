// One temporary canvas bends the existing pixels; PDF rendering stays off the animation path.
export class PaperTurn {
  constructor(host) { this.host = host; }

  finish() { this.complete?.(); }

  play(previous, next, direction) {
    this.finish();
    if (!direction || matchMedia('(prefers-reduced-motion: reduce)').matches) { next.inert = false; return Promise.resolve(); }
    const hostRect = this.host.getBoundingClientRect();
    const sheets = [...previous.querySelectorAll('.pdf-page')].map(element => ({
      canvas: element.querySelector('canvas'), rect: element.getBoundingClientRect(),
    })).sort((a, b) => a.rect.left - b.rect.left);
    const incoming = [...next.querySelectorAll('.pdf-page')].map(element => ({
      canvas: element.querySelector('canvas'), rect: element.getBoundingClientRect(),
    })).sort((a, b) => a.rect.left - b.rect.left);
    if (!hostRect.width || !hostRect.height || !sheets.length || sheets.length !== incoming.length || sheets.some(sheet => !sheet.canvas?.width)) {
      next.inert = false;
      return Promise.resolve();
    }
    const spread = sheets.length === 2 && incoming.length === 2;
    const moving = sheets[direction > 0 ? sheets.length - 1 : 0];
    const companion = spread ? sheets[direction > 0 ? 0 : 1] : null;
    const reverse = incoming[direction > 0 ? 0 : incoming.length - 1];
    const gutter = spread ? Math.max(0, sheets[1].rect.left - sheets[0].rect.right) : 0;
    // Mixed paper sizes cannot meet at one spine without stretching the new page.
    if (spread && [companion, ...incoming].some(sheet => Math.abs(moving.rect.width - sheet.rect.width) > 2 || Math.abs(moving.rect.height - sheet.rect.height) > 2)) {
      next.inert = false;
      return Promise.resolve();
    }
    const overlay = document.createElement('canvas');
    overlay.className = 'paper-turn';
    overlay.setAttribute('aria-hidden', 'true');
    const scale = Math.min(devicePixelRatio || 1, 2, Math.sqrt(6_000_000 / (hostRect.width * hostRect.height)));
    overlay.width = Math.ceil(hostRect.width * scale);
    overlay.height = Math.ceil(hostRect.height * scale);
    const ctx = overlay.getContext('2d');
    if (!ctx) { overlay.width = overlay.height = 0; next.inert = false; return Promise.resolve(); }
    const shadow = document.createElement('div');
    shadow.className = 'turn-shadow';
    shadow.setAttribute('aria-hidden', 'true');
    this.host.append(shadow, overlay);
    next.inert = true;
    this.host.dataset.turning = 'true';
    let frame;
    let done = false;
    return new Promise(resolve => {
      this.complete = () => {
        if (done) return;
        done = true;
        cancelAnimationFrame(frame);
        overlay.remove(); shadow.remove();
        overlay.width = overlay.height = 0;
        next.inert = false;
        delete this.host.dataset.turning;
        this.complete = null;
        resolve();
      };
      const start = performance.now();
      const paint = now => {
        if (done) return;
        const time = Math.min(1, (now - start) / 360);
        const progress = time * time * (3 - 2 * time);
        ctx.setTransform(scale, 0, 0, scale, 0, 0);
        ctx.clearRect(0, 0, hostRect.width, hostRect.height);
        if (companion) {
          const { canvas, rect } = companion;
          ctx.drawImage(canvas, rect.left - hostRect.left, rect.top - hostRect.top, rect.width, rect.height);
        }
        {
          const { canvas, rect } = moving;
          const width = rect.width, height = rect.height;
          const radius = width * .065 * Math.sin(Math.PI * progress);
          const fold = width * (1 - progress);
          const x = rect.left - hostRect.left, y = rect.top - hostRect.top;
          ctx.save();
          // A single-page viewport lets the turned sheet leave at its edge.
          if (!spread) {
            ctx.beginPath();
            ctx.rect(x - 5, y - 12, width + 10, height + 24);
            ctx.clip();
          }
          ctx.translate(direction > 0 ? x - gutter * progress : x + width + gutter * progress, y);
          if (direction < 0) ctx.scale(-1, 1);
          const project = sourceX => {
            const distance = Math.max(0, sourceX - fold);
            const theta = radius > .001 ? Math.min(Math.PI, distance / radius) : Math.PI;
            return sourceX <= fold ? { x: sourceX, z: 0, theta: 0 } : {
              x: fold + radius * Math.sin(theta) - Math.max(0, distance - Math.PI * radius),
              z: radius * (1 - Math.cos(theta)), theta,
            };
          };
          const drawBand = (from, to, left, span, top, tall, back) => {
            const texture = back && reverse?.canvas?.width ? reverse.canvas : canvas;
            const hasReverse = back && texture !== canvas;
            const reverseSource = hasReverse ? direction > 0 : direction < 0;
            const sourceX = texture.width * (reverseSource ? 1 - to / width : from / width);
            const sourceWidth = texture.width * (to - from) / width;
            if (back) { ctx.fillStyle = '#fff'; ctx.fillRect(left, top, span, tall); }
            ctx.save();
            ctx.globalAlpha = back && !hasReverse ? .055 : 1;
            if ((direction < 0) !== (back && !hasReverse)) {
              ctx.translate(left + span, top); ctx.scale(-1, 1);
              ctx.drawImage(texture, sourceX, 0, sourceWidth, texture.height, 0, 0, span, tall);
            } else {
              ctx.drawImage(texture, sourceX, 0, sourceWidth, texture.height, left, top, span, tall);
            }
            ctx.restore();
          };
          const flat = Math.max(0, Math.min(width, fold));
          if (flat > 0) drawBand(0, flat, 0, flat, 0, height, false);
          const bendEnd = Math.min(width, fold + Math.PI * radius);
          if (bendEnd < width) {
            const left = project(width).x;
            const span = project(bendEnd).x - left;
            const lift = radius * .4 / width;
            ctx.save();
            ctx.shadowColor = `rgba(30,26,19,${.18 * Math.sin(Math.PI * progress)})`;
            ctx.shadowBlur = 12;
            ctx.shadowOffsetX = -3;
            ctx.fillStyle = '#fff';
            ctx.fillRect(left, -height * lift / 2, span, height * (1 + lift));
            ctx.restore();
            drawBand(bendEnd, width, left, span, -height * lift / 2, height * (1 + lift), true);
            ctx.fillStyle = 'rgba(47,40,24,.035)';
            ctx.fillRect(left, -height * lift / 2, span, height * (1 + lift));
          }
          const strips = Math.max(12, Math.ceil((bendEnd - flat) / width * 96));
          for (let i = 0; i < strips && bendEnd > flat; i++) {
            const from = flat + (bendEnd - flat) * i / strips, to = flat + (bendEnd - flat) * (i + 1) / strips;
            const a = project(from), b = project(to);
            const left = Math.min(a.x, b.x), span = Math.abs(b.x - a.x) + .65;
            const lift = (a.z + b.z) / (width * 10);
            const top = -height * lift / 2;
            const tall = height * (1 + lift);
            const back = (a.theta + b.theta) / 2 > Math.PI / 2;
            drawBand(from, to, left, span, top, tall, back);
            const shade = back ? .035 + .12 * Math.sin((a.theta + b.theta) / 2) : .16 * Math.sin((a.theta + b.theta) / 2);
            ctx.fillStyle = `rgba(47,40,24,${shade})`;
            ctx.fillRect(left, top, span, tall);
          }
          ctx.restore();
        }
        const edge = moving.rect;
        const foldX = direction > 0 ? edge.left + edge.width * (1 - progress) - gutter * progress : edge.right - edge.width * (1 - progress) + gutter * progress;
        shadow.style.cssText = `left:${foldX - hostRect.left - 12}px;top:${edge.top - hostRect.top}px;width:${32 + Math.sin(Math.PI * progress) * 32}px;height:${edge.height}px;opacity:${Math.sin(Math.PI * progress) * .24};transform:translateX(${direction > 0 ? '0' : '-70%'})`;
        if (time < 1) frame = requestAnimationFrame(paint);
        else this.finish();
      };
      paint(start);
    });
  }
}
