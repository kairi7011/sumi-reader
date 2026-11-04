const { app, clipboard, ClipboardItem, nativeImage, dialog } = require('electron');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { fixturePDF } = require('./create-fixture.cjs');

const artifacts = path.resolve(__dirname, '../test-artifacts');
const userData = path.join(artifacts, 'user-data');
fs.mkdirSync(artifacts, { recursive: true });
fs.rmSync(userData, { recursive: true, force: true });
fs.mkdirSync(userData, { recursive: true });
app.setPath('userData', userData);
const fixture = fixturePDF(path.join(artifacts, 'Sumi reading notes.pdf'));
execFileSync('/usr/bin/swift', [path.join(__dirname, 'create-japanese-fixture.swift'), artifacts], { stdio: 'ignore' });
const fixtureByName = name => path.join(artifacts, fs.readdirSync(artifacts).find(entry => entry.normalize('NFC') === name));
const japaneseFixture = fixtureByName('日本語の読書テスト.pdf');
const passwordFixture = fixtureByName('パスワード付き.pdf');
const failures = [];
const checks = [];
const observations = {};
let restoreClipboard;

const sleep = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
async function waitFor(description, callback, timeout = 15000) {
  const end = Date.now() + timeout;
  let latest;
  while (Date.now() < end) {
    try { latest = await callback(); if (latest) return latest; } catch (error) { latest = error.message; }
    await sleep(60);
  }
  throw new Error(`Timed out: ${description} (${JSON.stringify(latest)})`);
}

async function run(window) {
  const contents = window.webContents;
  const js = source => contents.executeJavaScript(source, true);
  const click = selector => js(`document.querySelector(${JSON.stringify(selector)}).click()`);
  const select = (selector, value) => js(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); el.value = ${JSON.stringify(value)}; el.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  const check = (label, condition) => { assert.ok(condition, label); checks.push(label); console.log(`PASS ${label}`); };
  const readyPage = async page => {
    try { return await waitFor(`render page ${page}`, () => js(`document.querySelector('#page-input').value === '${page}' && document.querySelectorAll('#pages canvas').length > 0 && [...document.querySelectorAll('#pages canvas')].every(c => c.width > 0) && document.querySelector('#busy').hidden`)); }
    catch (error) { console.error('Reader state:', await js(`({page: document.querySelector('#page-input').value, pages: [...document.querySelectorAll('.pdf-page')].map(p => p.dataset.pageNumber), canvas: document.querySelectorAll('#pages canvas').length, busy: document.querySelector('#busy').hidden, status: document.querySelector('#reader-status').textContent})`)); throw error; }
  };

  await waitFor('renderer API', () => js(`Boolean(window.desktop && document.querySelector('#welcome'))`));
  check('welcome shown with no prior books', await js(`!document.querySelector('#welcome').hidden`));
  fs.writeFileSync(path.join(artifacts, 'welcome.png'), (await contents.capturePage()).toPNG());
  const preferences = contents.getLastWebPreferences();
  check('renderer has no Node access and remains sandboxed', preferences.sandbox && preferences.contextIsolation && !preferences.nodeIntegration && await js(`typeof require === 'undefined'`));
  check('shell API rejects malformed image without writing clipboard', await js(`window.desktop.copyImage('data:image/png;base64,AAAA').then(() => false, () => true)`));
  check('private shell files cannot be fetched', await js(`fetch('app://sumi/electron/main.cjs').then(r => r.status === 404)`));

  const nativeOpenDialog = dialog.showOpenDialog;
  let pickerCalls = 0;
  let completePicker;
  dialog.showOpenDialog = () => { pickerCalls++; return new Promise(resolve => { completePicker = resolve; }); };
  try {
    const simultaneousRequests = js(`Promise.all([window.desktop.openPdf(), window.desktop.openPdf()])`);
    await waitFor('shared picker request', () => pickerCalls > 0);
    await js('window.desktop.getLibrary()');
    check('simultaneous open requests share one native picker', pickerCalls === 1);
    completePicker({ canceled: true, filePaths: [] });
    const results = await simultaneousRequests;
    check('picker cancellation returns cleanly to both callers', results.length === 2 && results.every(result => result === null));
  } finally {
    completePicker?.({ canceled: true, filePaths: [] });
    dialog.showOpenDialog = nativeOpenDialog;
  }

  const previousClipboard = await Promise.all((await clipboard.read()).map(async item => new ClipboardItem(Object.fromEntries(await Promise.all(item.types.map(async type => [type, await item.getType(type)]))))));
  restoreClipboard = () => previousClipboard.length ? clipboard.write(previousClipboard) : clipboard.clear();

  app.emit('open-file', { preventDefault() {} }, fixture);
  await readyPage(1);
  check('native open-file renders all six PDF pages', await js(`document.querySelector('#page-count').textContent.trim() === '6' && document.querySelector('#welcome').hidden`));
  await waitFor('selectable text', () => js(`document.querySelectorAll('#pages .textLayer span').length > 5`));
  check('PDF text has a selectable text layer', true);
  fs.writeFileSync(path.join(artifacts, 'reader.png'), (await contents.capturePage()).toPNG());

  await click('#next-page');
  await readyPage(2);
  check('next-page control turns the page', true);
  await click('#copy-page');
  await waitFor('page text clipboard', async () => (await clipboard.readText()).includes('curiosity'));
  check('page text reaches native clipboard', true);

  const selection = await js(`(() => { const span = [...document.querySelectorAll('#pages .textLayer span')].find(el => el.textContent.includes('curiosity')); const range = document.createRange(); range.selectNodeContents(span); const selected = getSelection(); selected.removeAllRanges(); selected.addRange(range); return selected.toString(); })()`);
  contents.copy();
  await waitFor('selected text clipboard', async () => (await clipboard.readText()).includes(selection));
  check('native text selection copies the selected passage', selection.length > 5);
  await js('getSelection().removeAllRanges()');

  await click('#bookmark-toggle');
  await waitFor('bookmark persisted', () => js(`window.desktop.getLibrary().then(l => l.recent[0].state.bookmarks?.includes(2))`));
  check('bookmark is saved for the document', true);

  await click('#capture-region');
  const bounds = await js(`(() => { const r = document.querySelector('#pages canvas').getBoundingClientRect(); return { x: Math.round(r.x + 30), y: Math.round(Math.max(r.y + 40, 160)), width: Math.min(220, Math.round(r.width - 60)), height: 130 }; })()`);
  contents.sendInputEvent({ type: 'mouseMove', x: bounds.x, y: bounds.y });
  contents.sendInputEvent({ type: 'mouseDown', button: 'left', clickCount: 1, x: bounds.x, y: bounds.y });
  for (let index = 1; index <= 8; index++) {
    contents.sendInputEvent({ type: 'mouseMove', button: 'left', x: bounds.x + Math.round(bounds.width * index / 8), y: bounds.y + Math.round(bounds.height * index / 8) });
    await sleep(20);
  }
  contents.sendInputEvent({ type: 'mouseUp', button: 'left', clickCount: 1, x: bounds.x + bounds.width, y: bounds.y + bounds.height });
  await waitFor('captured image clipboard', () => clipboard.has('image/png'));
  const imageItem = (await clipboard.read()).find(item => item.types.includes('image/png'));
  const capture = nativeImage.createFromBuffer(Buffer.from(await (await imageItem.getType('image/png')).arrayBuffer()));
  check('drag capture produces a PNG on native clipboard', capture.getSize().width > 10 && capture.getSize().height > 10);
  fs.writeFileSync(path.join(artifacts, 'capture.png'), capture.toPNG());

  await select('#layout-select', 'spread');
  await waitFor('two-page spread', () => js(`document.querySelectorAll('#pages canvas').length === 2 && document.querySelector('#busy').hidden`));
  check('spread renders two pages', true);
  fs.writeFileSync(path.join(artifacts, 'spread.png'), (await contents.capturePage()).toPNG());
  await select('#layout-select', 'single');
  await waitFor('single-page render', () => js(`document.querySelectorAll('#pages canvas').length === 1 && document.querySelector('#busy').hidden`));

  await click('#search-toggle');
  await js(`(() => { const input = document.querySelector('#search-input'); input.value = 'curiosity'; input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await waitFor('search results', () => js(`!document.querySelector('#search-next').disabled`));
  check('full-document search finds fixture text', true);
  await click('#search-next');
  await readyPage(3);
  check('search next remains usable', await js(`!document.querySelector('#search-next').disabled`));
  await click('#search-toggle');
  await waitFor('search closed', () => js(`document.querySelector('#search-panel').hidden`));

  await select('#theme-select', 'night');
  check('night theme changes the reader', await js(`document.body.dataset.theme === 'night'`));
  await select('#theme-select', 'paper');
  await js(`(() => { const input = document.querySelector('#page-input'); input.value = '4'; input.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await readyPage(4);
  await waitFor('reading location persisted', () => js(`window.desktop.getLibrary().then(l => l.recent[0].state.page === 4)`));
  await new Promise(resolve => { contents.once('did-finish-load', resolve); contents.reload(); });
  await readyPage(4);
  check('reload restores the last reading position', true);
  check('reload restores bookmark', await js(`window.desktop.getLibrary().then(l => l.recent[0].state.bookmarks.includes(2))`));

  const id = await js(`window.desktop.getLibrary().then(l => l.recent[0].id)`);
  fs.renameSync(fixture, `${fixture}.moved`);
  try {
    check('missing recent file returns a useful error', await js(`window.desktop.reopenPdf(${JSON.stringify(id)}).then(() => false, e => e.message.includes('見つかりません'))`));
  } finally { fs.renameSync(`${fixture}.moved`, fixture); }
  fs.writeFileSync(path.join(artifacts, 'final-reader.png'), (await contents.capturePage()).toPNG());
  await click('#copy-page');
  await waitFor('clip collected', () => js(`document.querySelectorAll('#clips-list .clip-card').length > 0`));
  if (await js(`document.querySelector('#clips-panel').hidden`)) await click('#clips-toggle');
  window.setSize(900, 650);
  await waitFor('compact window', () => js(`innerWidth === 900 && document.querySelector('#busy').hidden`));
  await sleep(350);
  fs.writeFileSync(path.join(artifacts, 'compact-reader-clips.png'), (await contents.capturePage()).toPNG());
  check('compact reader with clips keeps page navigation reachable', await js(`(() => { const r = document.querySelector('#next-page').getBoundingClientRect(); return !document.querySelector('#clips-panel').hidden && r.x >= 0 && r.right <= innerWidth && r.y >= 0 && r.bottom <= innerHeight; })()`));
  window.setSize(1380, 940);
  await click('#clips-toggle');

  app.emit('open-file', { preventDefault() {} }, japaneseFixture);
  await readyPage(1);
  await waitFor('Japanese text layer', () => js(`document.querySelector('#pages').textContent.includes('日本語')`));
  check('Japanese PDF text renders as selectable text', true);
  await click('#copy-page');
  await waitFor('Japanese copy', async () => (await clipboard.readText()).includes('日本語の文章を読む'));
  check('Japanese text copies without spaces inserted between characters', true);
  fs.writeFileSync(path.join(artifacts, 'japanese-reader.png'), (await contents.capturePage()).toPNG());

  await click('#search-toggle');
  await js(`(() => { const input = document.querySelector('#search-input'); input.value = '第3章'; input.dispatchEvent(new Event('input', { bubbles: true })); input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); })()`);
  await sleep(350);
  check('Escape cancels a pending search without navigating', await js(`document.querySelector('#search-panel').hidden && document.querySelector('#page-input').value === '1'`));
  await click('#search-toggle');
  await js(`(() => { const input = document.querySelector('#search-input'); input.value = '日本語'; input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await waitFor('Japanese search results', () => js(`!document.querySelector('#search-next').disabled && document.querySelector('#search-status').textContent.includes('3')`));
  check('Japanese search finds the three fixture pages', true);
  await click('#search-toggle');

  check('capture remains unavailable while a new page is rendering', await js(`(() => { const input = document.querySelector('#page-input'); input.value = '2'; input.dispatchEvent(new Event('change', { bubbles: true })); document.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', metaKey: true, shiftKey: true, bubbles: true })); return document.querySelector('#capture-region').disabled && document.querySelectorAll('.capture-overlay').length === 0; })()`));
  await readyPage(2);
  const memorySnapshot = () => app.getAppMetrics().map(process => ({ type: process.type, workingSetKB: process.memory.workingSetSize }));
  observations.memoryBeforeNavigation = memorySnapshot();
  await js(`(() => { const input = document.querySelector('#page-input'); for (let page = 1; page <= 10; page++) { input.value = String(page % 3 + 1); input.dispatchEvent(new Event('change', { bubbles: true })); } })()`);
  await readyPage(2);
  observations.memoryAfterTenNavigationRequests = memorySnapshot();
  observations.aggregateWorkingSetKB = observations.memoryAfterTenNavigationRequests.reduce((total, process) => total + process.workingSetKB, 0);
  observations.renderedPagesAfterNavigation = await js(`document.querySelectorAll('.pdf-page').length`);
  check('rapid navigation settles with only the visible page rendered', observations.renderedPagesAfterNavigation === 1);
  await js(`(() => { const input = document.querySelector('#page-input'); input.value = '1'; input.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  await readyPage(1);

  app.emit('open-file', { preventDefault() {} }, passwordFixture);
  await waitFor('password dialog', () => js(`document.querySelector('#password-dialog').open`));
  await js(`(() => { document.querySelector('#password-input').value = 'wrong'; document.querySelector('#password-form').requestSubmit(); })()`);
  await waitFor('incorrect password feedback', () => js(`document.querySelector('#password-dialog').open && !document.querySelector('#password-error').hidden`));
  check('incorrect PDF password gets recoverable feedback', true);
  await js(`(() => { document.querySelector('#password-input').value = 'sumi'; document.querySelector('#password-form').requestSubmit(); })()`);
  await readyPage(1);
  check('correct password opens the protected PDF', await js(`!document.querySelector('#password-dialog').open && document.querySelector('#page-count').textContent === '3'`));

  app.emit('open-file', { preventDefault() {} }, passwordFixture);
  await waitFor('password dialog again', () => js(`document.querySelector('#password-dialog').open`));
  await click('#password-cancel');
  await waitFor('password cancel completes', () => js(`!document.querySelector('#password-dialog').open && document.querySelector('#busy').hidden`));
  check('password dialog can be cancelled', await js(`!document.querySelector('#welcome').hidden`));

  app.emit('open-file', { preventDefault() {} }, passwordFixture);
  await waitFor('password dialog before replacement', () => js(`document.querySelector('#password-dialog').open`));
  app.emit('open-file', { preventDefault() {} }, japaneseFixture);
  await readyPage(1);
  check('opening another PDF dismisses a previous password request', await js(`!document.querySelector('#password-dialog').open && document.querySelector('#document-title').textContent.normalize('NFC').includes('日本語')`));
  check('renderer remains responsive after all operations', await js(`document.readyState === 'complete'`));
}

app.once('browser-window-created', (_event, window) => {
  window.webContents.on('console-message', details => {
    if (details.level === 'error') failures.push(details.message);
  });
  window.webContents.once('did-finish-load', async () => {
    try {
      await run(window);
      await sleep(250);
      const report = { passed: checks.length, checks, rendererErrors: failures, observations };
      fs.writeFileSync(path.join(artifacts, 'smoke-report.json'), `${JSON.stringify(report, null, 2)}\n`);
      console.log(JSON.stringify(report, null, 2));
      await restoreClipboard?.();
      app.quit();
    } catch (error) {
      console.error(error);
      console.error('Renderer errors:', failures);
      try { fs.writeFileSync(path.join(artifacts, 'failure.png'), (await window.webContents.capturePage()).toPNG()); } catch {}
      await restoreClipboard?.();
      app.exit(1);
    }
  });
});

setTimeout(async () => { console.error('Smoke test exceeded 120 seconds.'); await restoreClipboard?.(); app.exit(1); }, 120_000).unref();
require('../electron/main.cjs');
