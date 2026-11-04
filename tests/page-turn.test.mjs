import test from 'node:test';
import assert from 'node:assert/strict';
import { PaperTurn } from '../src/page-turn.js';

function fixture({ reduced = false, width = 1200, height = 900, spread = false } = {}) {
  const originals = new Map();
  const install = (name, value) => { originals.set(name, Object.getOwnPropertyDescriptor(globalThis, name)); Object.defineProperty(globalThis, name, { value, configurable: true, writable: true }); };
  let serial = 0, clock = 0;
  const frames = new Map();
  const sampledPixels = [];
  const context = new Proxy({ drawImage(...args) { sampledPixels.push(args.length === 9 ? args[3] * args[4] : args[0].width * args[0].height); } }, { get: (target, key) => target[key] ?? (key === 'createLinearGradient' ? () => ({ addColorStop() {} }) : () => {}) });
  const host = { dataset: {}, children: [], getBoundingClientRect: () => ({ left: 0, top: 0, width, height }), append(canvas) { this.children.push(canvas); } };
  const batch = { inert: true, querySelectorAll: () => Array.from({ length: spread ? 2 : 1 }, (_, index) => ({
    querySelector: () => ({ width: 800, height: 1200 }),
    getBoundingClientRect: () => ({ left: 100 + index * 416, right: 500 + index * 416, top: 30, width: 400, height: 600 }),
  })) };
  const next = { ...batch };
  install('requestAnimationFrame', callback => { frames.set(++serial, callback); return serial; });
  install('cancelAnimationFrame', id => frames.delete(id));
  install('performance', { now: () => clock });
  install('matchMedia', () => ({ matches: reduced }));
  install('devicePixelRatio', 2);
  install('document', { createElement: () => ({ width: 0, height: 0, setAttribute() {}, getContext: () => context, remove() { host.children = host.children.filter(child => child !== this); } }) });
  return {
    host, next, previous: batch, frames, sampledPixels, turn: new PaperTurn(host),
    advance(time = 1000) { clock += time; const queued = [...frames.values()]; frames.clear(); queued.forEach(callback => callback(clock)); },
    restore() { for (const [name, descriptor] of originals) if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; },
  };
}

test('a corner drag can return to the old page and release its canvas and pending frame', async () => {
  const f = fixture({ spread: true });
  try {
    const session = f.turn.createSession(f.previous, f.next, 1, { cornerY: .1 });
    session.update(.3, 24); session.update(.42, 40);
    assert.equal(f.frames.size, 1, 'pointer moves share one frame');
    f.advance();
    assert.equal(f.host.children.length, 1);
    assert.equal(f.next.inert, true);
    const overlay = f.host.children[0];
    const result = session.settle(false, -.7);
    f.advance();
    assert.equal(await result, false);
    assert.equal(await session.finished, false);
    assert.equal(f.host.children.length, 0);
    assert.equal(overlay.width * overlay.height, 0, 'the bitmap is released');
    assert.equal(f.next.inert, false);
    assert.equal(f.frames.size, 0);
    assert.deepEqual(f.host.dataset, {});
  } finally { f.restore(); }
});

test('interrupting a turn commits once and leaves at most one bounded bitmap', async () => {
  const f = fixture({ width: 4000, height: 3000, spread: true });
  try {
    const first = f.turn.createSession(f.previous, f.next, -1);
    first.update(.6);
    const oldFrame = f.frames.values().next().value;
    const second = f.turn.createSession(f.previous, f.next, 1);
    assert.equal(await first.finished, true);
    assert.equal(f.host.children.length, 1);
    assert.ok(f.host.children[0].width * f.host.children[0].height <= 6_000_000);
    oldFrame?.(9999);
    f.turn.finish(); f.turn.finish();
    assert.equal(await second.finished, true);
    assert.equal(f.host.children.length, 0);
    assert.equal(f.next.inert, false);
  } finally { f.restore(); }
});

test('reduced motion still supports rollback and clears a staged batch inert flag', async () => {
  const f = fixture({ reduced: true });
  try {
    const session = f.turn.createSession(f.previous, f.next, 1);
    session.update(.6, 30);
    assert.equal(await session.settle(false), false);
    assert.equal(f.host.children.length, 0);
    assert.equal(f.frames.size, 0);
    assert.equal(f.next.inert, false);
    assert.equal(await f.turn.play(f.previous, f.next, -1), true);
  } finally { f.restore(); }
});

test('a curled frame samples a bounded amount of the original page textures', () => {
  const f = fixture({ spread: true });
  try {
    const session = f.turn.createSession(f.previous, f.next, 1);
    f.sampledPixels.length = 0;
    session.update(.65, -50); f.advance();
    const pagePixels = 800 * 1200;
    assert.ok(f.sampledPixels.reduce((sum, area) => sum + area, 0) < pagePixels * 12, 'texture work must not scale as a full page for every surface triangle');
    session.finish();
  } finally { f.restore(); }
});
