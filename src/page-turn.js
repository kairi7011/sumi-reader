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
    }));
    if (!sheets.length || sheets.some(sheet => !sheet.canvas?.width)) return Promise.resolve();
    const overlay = document.createElement('canvas');
    overlay.className = 'paper-turn';
    overlay.setAttribute('aria-hidden', 'true');
    const scale = Math.min(devicePixelRatio || 1, 2, Math.sqrt(6_000_000 / (hostRect.width * hostRect.height)));
    overlay.width = Math.ceil(hostRect.width * scale);
    overlay.height = Math.ceil(hostRect.height * scale);
    const ctx = overlay.getContext('2d');
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
        for (const { canvas, rect } of sheets) {
          const width = rect.width, height = rect.height;
          const radius = width * .065 * Math.sin(Math.PI * progress);
          const fold = width - progress * width * 1.1;
          const x = rect.left - hostRect.left, y = rect.top - hostRect.top;
          ctx.save();
          ctx.translate(direction > 0 ? x : x + width, y);
          if (direction < 0) ctx.scale(-1, 1);
          const strips = 96;
          const project = sourceX => {
            const distance = Math.max(0, sourceX - fold);
            const theta = radius > .01 ? Math.min(Math.PI, distance / radius) : 0;
            return sourceX <= fold ? { x: sourceX, z: 0, theta: 0 } : {
              x: fold + radius * Math.sin(theta) - Math.max(0, distance - Math.PI * radius),
              z: radius * (1 - Math.cos(theta)), theta,
            };
          };
          for (let i = 0; i < strips; i++) {
            const from = width * i / strips, to = width * (i + 1) / strips;
            const a = project(from), b = project(to);
            const left = Math.min(a.x, b.x), span = Math.abs(b.x - a.x) + .65;
            const lift = (a.z + b.z) / (width * 10);
            const top = -height * lift / 2;
            const tall = height * (1 + lift);
            const back = (a.theta + b.theta) / 2 > Math.PI / 2;
            ctx.fillStyle = '#fff';
            ctx.fillRect(left, top, span, tall);
            if (!back) {
              ctx.save();
              if (direction < 0) {
                ctx.translate(left + span, top); ctx.scale(-1, 1);
                ctx.drawImage(canvas, canvas.width * (1 - (i + 1) / strips), 0, canvas.width / strips, canvas.height, 0, 0, span, tall);
              } else {
                ctx.drawImage(canvas, canvas.width * i / strips, 0, canvas.width / strips, canvas.height, left, top, span, tall);
              }
              ctx.restore();
            }
            const shade = back ? .035 + .12 * Math.sin((a.theta + b.theta) / 2) : .16 * Math.sin((a.theta + b.theta) / 2);
            ctx.fillStyle = `rgba(47,40,24,${shade})`;
            ctx.fillRect(left, top, span, tall);
          }
          ctx.restore();
        }
        const edge = sheets[direction > 0 ? sheets.length - 1 : 0].rect;
        const foldX = direction > 0 ? edge.left + edge.width * (1 - progress * 1.1) : edge.right - edge.width * (1 - progress * 1.1);
        shadow.style.cssText = `left:${foldX - hostRect.left - 12}px;top:${edge.top - hostRect.top}px;width:${32 + Math.sin(Math.PI * progress) * 32}px;height:${edge.height}px;opacity:${Math.sin(Math.PI * progress) * .24};transform:translateX(${direction > 0 ? '0' : '-70%'})`;
        if (time < 1) frame = requestAnimationFrame(paint);
        else this.finish();
      };
      paint(start);
    });
  }
}
