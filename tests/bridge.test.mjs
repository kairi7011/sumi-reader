import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

function bridge() {
  const listeners = new Map(), reads = new Map(), calls = [];
  const window = { __TAURI__: {
    core: { invoke: async (command, args) => {
      calls.push(command);
      if (command === 'reopen_pdf') return { id: args.id };
      if (command === 'read_pdf') return new Promise(resolve => reads.set(args.id, resolve));
    } },
    event: { listen: async (name, callback) => { listeners.set(name, callback); return () => listeners.delete(name); } },
  } };
  vm.runInNewContext(readFileSync(new URL('../src/desktop.js', import.meta.url), 'utf8'), { window });
  return { api: window.desktop, listeners, reads, calls };
}

test('a slow older native file cannot replace the latest selection', async () => {
  const { api, listeners, reads } = bridge();
  const shown = [];
  api.onOpenPdf(document => shown.push(document.id));
  const first = listeners.get('pdf-opened')({ payload: { id: 'first' } });
  const last = listeners.get('pdf-opened')({ payload: { id: 'last' } });
  assert.equal(api.hasPendingOpen, true);
  reads.get('last')(new ArrayBuffer(3));
  await last;
  reads.get('first')(new ArrayBuffer(3));
  await first;
  assert.deepEqual(shown, ['last']);
  assert.equal(api.hasPendingOpen, false);
});

test('recent-book requests discard an obsolete byte response', async () => {
  const { api, reads } = bridge();
  const first = api.reopenPdf('first');
  const last = api.reopenPdf('last');
  await new Promise(setImmediate);
  reads.get('last')(new ArrayBuffer(2));
  assert.equal((await last).id, 'last');
  reads.get('first')(new ArrayBuffer(2));
  assert.equal(await first, null);
});

test('startup installs event listeners before asking native code to drain queued files', async () => {
  const { api, calls, listeners } = bridge();
  api.onOpenPdf(() => {});
  api.onOpenError(() => {});
  api.onCommand(() => {});
  await api.ready();
  assert.ok(listeners.has('pdf-opened') && listeners.has('pdf-error') && listeners.has('app-command'));
  assert.deepEqual(calls, ['frontend_ready']);
});
