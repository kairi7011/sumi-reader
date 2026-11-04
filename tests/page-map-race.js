(async () => {
  const invoke = window.__TAURI__.core.invoke;
  const $ = selector => document.querySelector(selector);
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const traces = [], checks = [], observations = {};
  let animationFrames = 0, lastAnimationFrame = performance.now(); const watchFrames = () => { animationFrames++; lastAnimationFrame = performance.now(); requestAnimationFrame(watchFrames); }; requestAnimationFrame(watchFrames);
  const pages = new Map();
  const started = performance.now();
  const trace = (phase, details = {}) => { traces.push({ ms: Math.round(performance.now() - started), phase, ...details }); if (traces.length > 1400) traces.shift(); };
  const state = () => ({ animationFrames, lastAnimationFrameAgoMs: Math.round(performance.now() - lastAnimationFrame), visibility: document.visibilityState, tasks: [...pages].map(([number, page]) => ({ page: number, intents: [...(page._intentStates?.values() || [])].map(intent => ({ last: intent.operatorList?.lastChunk, stream: Boolean(intent.streamReader), tasks: [...(intent.renderTasks || [])].map(task => ({ index: task.operatorListIdx, nextOp: task.operatorList?.fnArray?.[task.operatorListIdx], ready: task.graphicsReady, running: task.running, cancelled: task.cancelled })) })) })), requested: $('#page-input')?.value, visible: [...document.querySelectorAll('#pages .pdf-page')].map(node => node.dataset.pageNumber), staging: [...document.querySelectorAll('.staging-pages .pdf-page')].map(node => ({ page: node.dataset.pageNumber, canvas: node.querySelector('canvas')?.width, text: node.querySelector('.textLayer')?.childElementCount })), loading: $('#reader')?.dataset.loading, busy: $('#busy-label')?.textContent });
  const wait = async (label, test, limit = 10000) => { const end = performance.now() + limit; while (performance.now() < end) { if (test()) return; await sleep(25); } throw new Error(`${label}: ${JSON.stringify(state())}`); };
  const settled = page => wait(`page ${page}`, () => $('#page-input').value === String(page) && $('#pages .pdf-page')?.dataset.pageNumber === String(page) && $('#reader').dataset.loading !== 'true' && $('#reading-area').dataset.turning !== 'true' && !$('#capture-region').disabled);
  const navigate = page => { $('#page-input').value = page; $('#page-input').dispatchEvent(new Event('change', { bubbles: true })); };
  const wrapped = new WeakSet();
  const { PageMap } = await import('./page-map.js');
  const open = PageMap.prototype.open;
  let map;
  PageMap.prototype.open = function () {
    map = this;
    const pdf = this.getDocument();
    if (!wrapped.has(pdf)) {
      wrapped.add(pdf);
      const getPage = pdf.getPage.bind(pdf);
      pdf.getPage = async number => {
        if (number === 200 || number === 201) trace('getPage begin', { page: number });
        const page = await getPage(number);
        if (number === 200 || number === 201) trace('getPage done', { page: number });
        if ((number === 200 || number === 201) && !wrapped.has(page)) {
          wrapped.add(page); pages.set(number, page);
          const render = page.render.bind(page), cleanup = page.cleanup.bind(page), text = page.streamTextContent.bind(page);
          const intents = () => [...(page._intentStates?.values() || [])].map(intent => ({ tasks: intent.renderTasks?.size, last: intent.operatorList?.lastChunk, abort: Boolean(intent.streamReaderCancelTimeout), stream: Boolean(intent.streamReader) }));
          page.render = options => {
            const owner = options.canvasContext.canvas.closest('.staging-pages') ? 'reader' : 'thumbnail';
            trace('render begin', { page: number, owner, intents: intents() });
            const task = render(options), cancel = task.cancel.bind(task);
            task.cancel = (...args) => { trace('render cancel', { page: number, owner }); return cancel(...args); };
            task.promise.then(() => trace('render done', { page: number, owner, intents: intents() }), error => trace('render rejected', { page: number, owner, error: error.name, intents: intents() }));
            return task;
          };
          page.cleanup = (...args) => { const before = intents(), result = cleanup(...args); trace('cleanup', { page: number, result, before, after: intents() }); return result; };
          page.streamTextContent = (...args) => { trace('text begin', { page: number }); return text(...args); };
        }
        return page;
      };
    }
    return open.call(this);
  };
  try {
    const [fixture] = await invoke('debug_smoke_info');
    await invoke('debug_open_fixture', { index: 0 });
    await wait('opened', () => $('#document-title').title.normalize('NFC') === fixture.name.normalize('NFC'));
    await settled(1);
    $('#layout-select').value = 'spread'; $('#layout-select').dispatchEvent(new Event('change', { bubbles: true })); await settled(1);
    for (const inputMode of ['direct', 'typed']) for (const delay of [0, 15, 50, 120, 250, 500, 1000, 2500]) {
      navigate(114); await settled(114); await sleep(100);
      if (map) { await wait('thumbnail jobs drain', () => map.jobs.size === 0); map.reset(); }
      $('#page-map-toggle').click(); await wait('map open', () => $('#page-map').open);
      for (const value of inputMode === 'typed' ? ['2', '20', '200'] : ['200']) {
        $('#page-map-input').value = value; $('#page-map-input').dispatchEvent(new InputEvent('input', { bubbles: true, data: value.at(-1), inputType: 'insertText' }));
        if (inputMode === 'typed') await sleep(30);
      }
      await sleep(delay);
      trace('commit', { inputMode, delay, previewLoading: $('#page-map-preview').dataset.loading, jobs: [...map.jobs.keys()] });
      $('#page-map-form').requestSubmit();
      try { await settled(200); checks.push(`${inputMode} spread jump with ${delay}ms preview delay`); }
      catch (error) {
        observations.failedInputMode = inputMode; observations.failedDelay = delay; observations.hungState = state();
        navigate(1); await settled(1); navigate(200); await settled(200);
        checks.push('direct page navigation recovers from the map hang');
        throw error;
      }
    }
    navigate(114); await settled(114);
    let peakJobs = 0;
    for (let round = 0; round < 16; round++) {
      $('#page-map-toggle').click(); await wait('rapid map open', () => $('#page-map').open);
      $('#page-map-input').value = String(200 + round); $('#page-map-input').dispatchEvent(new Event('input', { bubbles: true }));
      await sleep(8);
      peakJobs = Math.max(peakJobs, map.jobs.size);
      map.close();
      peakJobs = Math.max(peakJobs, map.jobs.size);
      if (peakJobs > 2) throw new Error('Rapid reopen exceeded two thumbnail jobs');
    }
    await wait('remaining thumbnail jobs finish', () => map.jobs.size === 0);
    observations.rapidReopens = 16; observations.peakThumbnailJobs = peakJobs;
    checks.push('rapid close and reopen retains at most two thumbnail jobs');
    $('#page-map-toggle').click(); await wait('final map open', () => $('#page-map').open);
    $('#page-map-input').value = '200'; $('#page-map-input').dispatchEvent(new Event('input', { bubbles: true }));
    $('#page-map-form').requestSubmit(); await settled(200);
    checks.push('immediate jump still works after rapid reopen stress');
    await invoke('debug_smoke_report', { report: { passed: true, checks, observations, traces }, snapshot: null });
  } catch (error) {
    await invoke('debug_smoke_report', { report: { passed: false, checks, observations, traces, error: String(error.message || error), state: state() }, snapshot: null });
  }
})();
