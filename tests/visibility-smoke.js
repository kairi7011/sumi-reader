(async () => {
  const invoke = window.__TAURI__.core.invoke;
  const $ = selector => document.querySelector(selector);
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const checks = [], observations = {};
  let frames = 0, lastFrame = performance.now();
  const heartbeat = () => { frames++; lastFrame = performance.now(); requestAnimationFrame(heartbeat); };
  requestAnimationFrame(heartbeat);
  const wait = async (label, predicate, limit = 10000) => { const end = performance.now() + limit; while (performance.now() < end) { if (predicate()) return; await sleep(25); } throw new Error(label); };
  const settled = page => wait(`page ${page}`, () => $('#pages .pdf-page')?.dataset.pageNumber === String(page) && $('#reader').dataset.loading !== 'true' && !$('#capture-region').disabled);
  try {
    const [fixture] = await invoke('debug_smoke_info');
    await invoke('debug_open_fixture', { index: 0 });
    await wait('document opens', () => $('#document-title').title.normalize('NFC') === fixture.name.normalize('NFC'));
    await settled(1);
    observations.visibilityBeforeHide = document.visibilityState;
    const hiddenAt = performance.now(), frameCount = frames;
    await invoke('debug_hide_window', { durationMs: 1800 });
    await sleep(150);
    observations.visibilityDuringHide = document.visibilityState;
    $('#page-input').value = '200'; $('#page-input').dispatchEvent(new Event('change', { bubbles: true }));
    await sleep(350);
    observations.hidden = { visibility: document.visibilityState, frameGapMs: Math.round(performance.now() - lastFrame), frames: frames - frameCount, renderPending: $('#reader').dataset.loading === 'true' };
    if (observations.hidden.visibility !== 'hidden' || observations.hidden.frameGapMs < 200) throw new Error('Hidden-window animation-frame suspension was not observed');
    checks.push('WebKit suspends animation frames while the native window is hidden');
    await settled(200);
    observations.restored = { visibility: document.visibilityState, elapsedMs: Math.round(performance.now() - hiddenAt), frameGapMs: Math.round(performance.now() - lastFrame), page: Number($('#pages .pdf-page').dataset.pageNumber) };
    if (observations.restored.visibility !== 'visible' || observations.restored.elapsedMs > 4000) throw new Error('Rendering did not resume promptly when the native window was raised');
    checks.push('raising the window resumes and completes the requested page');
    await invoke('debug_smoke_report', { report: { passed: true, checks, observations }, snapshot: null });
  } catch (error) {
    await invoke('debug_smoke_report', { report: { passed: false, checks, observations, error: String(error.message || error) }, snapshot: null });
  }
})();
