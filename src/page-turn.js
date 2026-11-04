const clamp = (value, low, high) => Math.max(low, Math.min(high, value));

function polygon(ctx, points) {
  ctx.beginPath();
  points.forEach((point, index) => index ? ctx.lineTo(point.x, point.y) : ctx.moveTo(point.x, point.y));
  ctx.closePath();
}

function clipPolygon(corners, distance) {
  const result = [];
  corners.forEach((a, index) => {
    const b = corners[(index + 1) % corners.length];
    const da = distance(a.x, a.y), db = distance(b.x, b.y);
    if (da <= 0) result.push(a);
    if ((da <= 0) !== (db <= 0)) {
      const t = da / (da - db);
      result.push({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
    }
  });
  return result;
}

function rectangle(width, top, bottom) {
  return [{ x: 0, y: top }, { x: width, y: top }, { x: width, y: bottom }, { x: 0, y: bottom }];
}

// Affine triangles approximate the bent surface without allocating another texture canvas.
function textureTriangle(ctx, texture, points, width, height, mirror, alpha = 1) {
  const uv = points.map(point => ({ x: (mirror ? 1 - point.u / width : point.u / width) * texture.width, y: point.v / height * texture.height }));
  const [a, b, c] = uv, [p, q, r] = points;
  const determinant = a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y);
  if (Math.abs(determinant) < .0001) return;
  const coefficient = key => [
    (p[key] * (b.y - c.y) + q[key] * (c.y - a.y) + r[key] * (a.y - b.y)) / determinant,
    (p[key] * (c.x - b.x) + q[key] * (a.x - c.x) + r[key] * (b.x - a.x)) / determinant,
    (p[key] * (b.x * c.y - c.x * b.y) + q[key] * (c.x * a.y - a.x * c.y) + r[key] * (a.x * b.y - b.x * a.y)) / determinant,
  ];
  const x = coefficient('x'), y = coefficient('y');
  const orientation = Math.sign((q.x - p.x) * (r.y - p.y) - (r.x - p.x) * (q.y - p.y)) || 1;
  const normals = points.map((point, index) => {
    const next = points[(index + 1) % 3], dx = next.x - point.x, dy = next.y - point.y;
    const length = Math.hypot(dx, dy) || 1;
    return { x: orientation * dy / length, y: -orientation * dx / length };
  });
  // Offset the edges, not radial vertices: narrow curl triangles otherwise leave white seams.
  const clip = points.map((point, index) => {
    const before = normals[(index + 2) % 3], after = normals[index];
    const x = before.x + after.x, y = before.y + after.y;
    const distance = Math.min(.65 / Math.max(.01, 1 + before.x * after.x + before.y * after.y), 3 / (Math.hypot(x, y) || 1));
    return { x: point.x + x * distance, y: point.y + y * distance };
  });
  ctx.save();
  polygon(ctx, clip); ctx.clip();
  const bounds = { x: Math.min(...clip.map(point => point.x)), y: Math.min(...clip.map(point => point.y)), width: Math.max(...clip.map(point => point.x)) - Math.min(...clip.map(point => point.x)), height: Math.max(...clip.map(point => point.y)) - Math.min(...clip.map(point => point.y)) };
  if (alpha < 1) { ctx.fillStyle = '#fffef9'; ctx.fillRect(bounds.x, bounds.y, bounds.width, bounds.height); }
  ctx.save();
  ctx.transform(x[0], y[0], x[1], y[1], x[2], y[2]);
  ctx.globalAlpha = alpha;
  // WebKit otherwise samples the entire page for every clipped triangle.
  const padding = Math.ceil(Math.max(texture.width / width, texture.height / height) * 2);
  const sourceX = Math.max(0, Math.floor(Math.min(a.x, b.x, c.x)) - padding);
  const sourceY = Math.max(0, Math.floor(Math.min(a.y, b.y, c.y)) - padding);
  const sourceWidth = Math.min(texture.width, Math.ceil(Math.max(a.x, b.x, c.x)) + padding) - sourceX;
  const sourceHeight = Math.min(texture.height, Math.ceil(Math.max(a.y, b.y, c.y)) + padding) - sourceY;
  ctx.drawImage(texture, sourceX, sourceY, sourceWidth, sourceHeight, sourceX, sourceY, sourceWidth, sourceHeight);
  ctx.restore();
  const shades = points.map(point => .018 * (1 - Math.cos(point.theta)) + .15 * Math.sin(point.theta));
  const area = (q.x - p.x) * (r.y - p.y) - (r.x - p.x) * (q.y - p.y);
  if (Math.abs(area) > .0001) {
    const gx = ((shades[1] - shades[0]) * (r.y - p.y) - (shades[2] - shades[0]) * (q.y - p.y)) / area;
    const gy = ((shades[2] - shades[0]) * (q.x - p.x) - (shades[1] - shades[0]) * (r.x - p.x)) / area;
    const norm = gx * gx + gy * gy;
    const low = Math.min(...shades), high = Math.max(...shades);
    if (norm > .00000001 && high - low > .001) {
      const gradient = ctx.createLinearGradient(p.x + (low - shades[0]) * gx / norm, p.y + (low - shades[0]) * gy / norm, p.x + (high - shades[0]) * gx / norm, p.y + (high - shades[0]) * gy / norm);
      gradient.addColorStop(0, `rgba(42,34,20,${low})`); gradient.addColorStop(1, `rgba(42,34,20,${high})`);
      ctx.fillStyle = gradient;
    } else ctx.fillStyle = `rgba(42,34,20,${shades[0]})`;
    ctx.fillRect(bounds.x, bounds.y, bounds.width, bounds.height);
  }
  ctx.restore();
}

export class PaperTurn {
  constructor(host) { this.host = host; }

  finish() { this.active?.finish(true); }

  play(previous, next, direction) {
    return this.createSession(previous, next, direction).settle(true);
  }

  createSession(previous, next, direction, { cornerY = .82 } = {}) {
    this.finish();
    let resolveFinished;
    const finished = new Promise(resolve => { resolveFinished = resolve; });
    let done = false, frame = 0, progress = 0, vertical = 0, settling = false;
    let overlay, paint = () => {};
    const complete = (commit = true) => {
      if (done) return;
      done = true;
      cancelAnimationFrame(frame);
      overlay?.remove();
      if (overlay) overlay.width = overlay.height = 0;
      next.inert = false;
      if (this.active === session) {
        delete this.host.dataset.turning;
        delete this.host.dataset.turnPhase;
        this.active = null;
      }
      resolveFinished(Boolean(commit));
    };
    const session = {
      finished,
      update: (value, offset = 0) => {
        if (done || settling) return;
        progress = clamp(Number.isFinite(value) ? value : 0, 0, 1);
        vertical = Number.isFinite(offset) ? offset : 0;
        if (!frame) frame = requestAnimationFrame(() => { frame = 0; if (!done) paint(); });
      },
      settle: (commit, velocity = 0) => {
        if (done || settling) return finished;
        settling = true;
        cancelAnimationFrame(frame); frame = 0;
        const target = commit ? 1 : 0, from = progress, startVertical = vertical;
        const travel = target - from;
        if (!overlay || Math.abs(travel) < .001) { complete(commit); return finished; }
        this.host.dataset.turnPhase = 'settling';
        const speed = Number.isFinite(velocity) ? velocity : 0;
        const duration = clamp(Math.abs(travel) * 560 / (1 + Math.abs(speed) * .12), 180, 580);
        const tangent = clamp(speed * duration / 1000 / travel, 0, 2.4);
        const start = performance.now();
        const tick = now => {
          if (done) return;
          const t = clamp((now - start) / duration, 0, 1);
          const eased = (t * t * (3 - 2 * t)) + tangent * t * (1 - t) * (1 - t);
          progress = from + travel * eased;
          vertical = startVertical * (1 - t * t * (3 - 2 * t));
          paint();
          if (t < 1) frame = requestAnimationFrame(tick);
          else complete(commit);
        };
        frame = requestAnimationFrame(tick);
        return finished;
      },
      finish: complete,
    };
    this.active = session;
    if (!direction || matchMedia('(prefers-reduced-motion: reduce)').matches) return session;
    direction = direction > 0 ? 1 : -1;
    cornerY = clamp(Number.isFinite(cornerY) ? cornerY : .82, 0, 1);
    const hostRect = this.host.getBoundingClientRect();
    const snapshot = batch => [...batch.querySelectorAll('.pdf-page')].map(element => ({
      canvas: element.querySelector('canvas'), rect: element.getBoundingClientRect(),
    })).sort((a, b) => a.rect.left - b.rect.left);
    const sheets = snapshot(previous), incoming = snapshot(next);
    if (!hostRect.width || !hostRect.height || !sheets.length || sheets.length !== incoming.length || sheets.some(sheet => !sheet.canvas?.width) || incoming.some(sheet => !sheet.canvas?.width)) return session;
    const spread = sheets.length === 2;
    const moving = sheets[direction > 0 ? sheets.length - 1 : 0];
    const companion = spread ? sheets[direction > 0 ? 0 : 1] : null;
    const reverse = spread ? incoming[direction > 0 ? 0 : incoming.length - 1] : null;
    if (spread && [companion, ...incoming].some(sheet => Math.abs(moving.rect.width - sheet.rect.width) > 2 || Math.abs(moving.rect.height - sheet.rect.height) > 2)) return session;
    const { canvas, rect } = moving;
    const width = rect.width, height = rect.height;
    if (!width || !height) return session;
    const gutter = spread ? Math.max(0, sheets[1].rect.left - sheets[0].rect.right) : 0;
    overlay = document.createElement('canvas');
    overlay.className = 'paper-turn'; overlay.setAttribute('aria-hidden', 'true');
    const scale = Math.min(devicePixelRatio || 1, 1.5, Math.sqrt(3_490_000 / (hostRect.width * hostRect.height)));
    overlay.width = Math.floor(hostRect.width * scale); overlay.height = Math.floor(hostRect.height * scale);
    const ctx = overlay.getContext('2d');
    if (!ctx) { overlay.width = overlay.height = 0; overlay = null; return session; }
    this.host.append(overlay);
    next.inert = true;
    this.host.dataset.turning = 'true'; this.host.dataset.turnPhase = 'dragging';
    const bendBands = 18, verticalBands = 8;
    const cornerSign = cornerY < .5 ? -1 : 1;
    const anchorY = cornerSign > 0 ? height : 0;
    paint = () => {
      const p = clamp(progress, 0, 1), lift = Math.sin(Math.PI * p);
      ctx.setTransform(scale, 0, 0, scale, 0, 0);
      ctx.clearRect(0, 0, hostRect.width, hostRect.height);
      if (companion) ctx.drawImage(companion.canvas, companion.rect.left - hostRect.left, companion.rect.top - hostRect.top, companion.rect.width, companion.rect.height);
      ctx.save();
      const originX = (direction > 0 ? rect.left : rect.right) - hostRect.left - direction * gutter * p;
      const originY = rect.top - hostRect.top;
      if (!spread) { ctx.beginPath(); ctx.rect(rect.left - hostRect.left - 1, originY - 18, width + 2, height + 36); ctx.clip(); }
      ctx.translate(originX, originY); ctx.scale(direction, 1);
      if (p < .0001) {
        ctx.save(); if (direction < 0) { ctx.translate(width, 0); ctx.scale(-1, 1); }
        ctx.drawImage(canvas, 0, 0, width, height); ctx.restore(); ctx.restore(); return;
      }
      const tilt = clamp(cornerSign * .43 - vertical / height * .8, -.8, .8) * Math.pow(1 - p, .72);
      const nx = Math.cos(tilt), ny = Math.sin(tilt);
      const fold = nx * width * Math.pow(1 - p, 1.12);
      const distance = (u, v) => nx * u + ny * (v - anchorY) - fold;
      const radius = width * (.045 + .065 * lift) * lift;
      const radiusAt = v => Math.max(.001, radius * (.72 + .43 * Math.abs(v - anchorY) / height));
      const project = (u, v) => {
        const s = distance(u, v);
        const r = radiusAt(v);
        const theta = s <= 0 ? 0 : Math.min(Math.PI, s / r);
        const along = s <= 0 ? s : r * Math.sin(theta) - Math.max(0, s - Math.PI * r);
        const z = s <= 0 ? 0 : r * (1 - Math.cos(theta));
        const perspective = 1 / (1 - z / (width * 5));
        return { u, v, x: (u + nx * (along - s)) * perspective, y: height / 2 + (v + ny * (along - s) - height / 2) * perspective, z, theta, s };
      };
      const outline = [];
      for (let edge = 0; edge < 4; edge++) for (let i = 0; i < 24; i++) {
        const t = i / 24;
        outline.push(project(edge === 0 ? t * width : edge === 1 ? width : edge === 2 ? (1 - t) * width : 0, edge === 0 ? 0 : edge === 1 ? t * height : edge === 2 ? height : (1 - t) * height));
      }
      ctx.save();
      ctx.shadowColor = `rgba(25,22,16,${.24 * lift})`; ctx.shadowBlur = 18 * lift;
      ctx.shadowOffsetX = -10 * lift; ctx.shadowOffsetY = 12 * lift;
      polygon(ctx, outline); ctx.fillStyle = '#fffef9'; ctx.fill(); ctx.restore();
      const flat = clipPolygon(rectangle(width, 0, height), distance);
      if (flat.length > 2) {
        ctx.save(); polygon(ctx, flat); ctx.clip();
        if (direction < 0) { ctx.translate(width, 0); ctx.scale(-1, 1); }
        ctx.drawImage(canvas, 0, 0, width, height); ctx.restore();
      }
      const triangles = [];
      const addSurface = (polygon, back) => {
        const projected = polygon.map(point => project(point.x, point.y));
        for (let i = 1; i + 1 < projected.length; i++) {
          const points = [projected[0], projected[i], projected[i + 1]];
          triangles.push({ points, back, z: points.reduce((sum, point) => sum + point.z, 0) / 3 });
        }
      };
      for (let row = 0; row < verticalBands; row++) {
        const top = height * row / verticalBands, bottom = height * (row + 1) / verticalBands;
        // The turned face is planar; a few horizontal slices preserve its perspective.
        addSurface(clipPolygon(rectangle(width, top, bottom), (u, v) => Math.PI * radiusAt(v) - distance(u, v)), true);
        for (let band = 0; band < bendBands; band++) {
          const from = Math.PI * band / bendBands, to = Math.PI * (band + 1) / bendBands;
          const source = (theta, v) => ({ x: (fold - ny * (v - anchorY) + theta * radiusAt(v)) / nx, y: v });
          let surface = [source(from, top), source(to, top), source(to, bottom), source(from, bottom)];
          surface = clipPolygon(surface, u => -u);
          surface = clipPolygon(surface, u => u - width);
          // An angle boundary at pi/2 prevents a triangle from mixing the two printed faces.
          addSurface(surface, band >= bendBands / 2);
        }
      }
      triangles.sort((a, b) => a.z - b.z);
      for (const triangle of triangles) {
        const back = triangle.back;
        const texture = back && reverse ? reverse.canvas : canvas;
        const mirror = back && reverse ? direction > 0 : direction < 0;
        textureTriangle(ctx, texture, triangle.points, width, height, mirror, back && !reverse ? .075 : 1);
      }
      polygon(ctx, outline); ctx.lineWidth = .75; ctx.strokeStyle = `rgba(65,55,38,${.12 + lift * .15})`; ctx.stroke();
      const freeEdge = Array.from({ length: 40 }, (_, i) => project(width, height * i / 39));
      ctx.beginPath(); freeEdge.forEach((point, index) => index ? ctx.lineTo(point.x, point.y) : ctx.moveTo(point.x, point.y));
      ctx.lineWidth = 1.3; ctx.strokeStyle = 'rgba(255,255,248,.8)'; ctx.stroke();
      ctx.restore();
    };
    paint();
    return session;
  }
}
