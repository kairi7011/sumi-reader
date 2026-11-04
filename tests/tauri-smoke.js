(async () => {
  const invoke = window.__TAURI__.core.invoke;
  const checks = [];
  const observations = { errors: [], userAgent: navigator.userAgent };
  let snapshot = null;
  const $ = selector => document.querySelector(selector);
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (label, condition) => { if (!condition) throw new Error(label); checks.push(label); };
  async function wait(label, predicate, timeout = 25000) {
    const until = performance.now() + timeout;
    while (performance.now() < until) { if (await predicate()) return; await sleep(30); }
    throw new Error(`Timed out: ${label}; page=${$('#page-input')?.value}; status=${$('#reader-status')?.textContent}`);
  }
  function navigate(page) { $('#page-input').value = String(page); $('#page-input').dispatchEvent(new Event('change', { bubbles: true })); }
  async function settled(page) {
    await wait(`page ${page}`, () => $('#page-input').value === String(page) && $('#pages .pdf-page')?.dataset.pageNumber === String(page) && $('#pages canvas')?.width > 0 && $('#reader').dataset.loading !== 'true' && $('#reading-area').dataset.turning !== 'true' && !$('#capture-region').disabled);
  }
  function select(selector, value) { $(selector).value = value; $(selector).dispatchEvent(new Event('change', { bubbles: true })); }
  window.addEventListener('error', event => observations.errors.push(String(event.message)));
  window.addEventListener('unhandledrejection', event => observations.errors.push(String(event.reason?.message || event.reason)));
  try {
    const fixtures = await invoke('debug_smoke_info');
    assert('native Tauri bridge is active', window.desktop?.native === 'tauri');
    for (let index = 0; index < fixtures.length; index++) {
      const fixture = fixtures[index];
      const protectedPdf = fixture.name.normalize('NFC').includes('パスワード');
      await invoke('debug_open_fixture', { index });
      if (protectedPdf) {
        await wait('password dialog', () => $('#password-dialog').open);
        $('#password-input').value = 'wrong'; $('#password-form').requestSubmit();
        await wait('wrong password feedback', () => $('#password-dialog').open && !$('#password-error').hidden);
        assert('wrong PDF password shows feedback', true);
        $('#password-input').value = 'sumi'; $('#password-form').requestSubmit();
      }
      await wait('document title', () => $('#document-title').textContent.normalize('NFC') === fixture.name.normalize('NFC'));
      await settled(1);
      await sleep(180); await settled(1);
      const total = Number($('#page-count').textContent);
      assert(`WKWebView rendered document ${index + 1}`, total > 0);
      const library = await window.desktop.getLibrary();
      const bytes = await invoke('read_pdf', { id: library.lastOpenedId });
      assert(`raw PDF IPC response ${index + 1}`, bytes instanceof ArrayBuffer && new Uint8Array(bytes)[0] === 37);
      if (fixture.name.normalize('NFC').includes('日本語')) {
        assert('Japanese selectable text is present', $('#pages').textContent.includes('日本語'));
        observations.copyButton = { disabled: $('#copy-page').disabled, busy: $('#copy-page').dataset.busy };
        $('#copy-page').click();
        await wait('Japanese clipboard', async () => { const text = await invoke('debug_clipboard_text'); observations.fixtureClipboard = text.slice(0, 1000); observations.copyBusy = $('#copy-page').dataset.busy; return text.includes('日本語の文章を読む'); });
        assert('Japanese text copied through the native clipboard', true);
        snapshot = $('#pages canvas').toDataURL('image/png');
        $('#search-toggle').click(); $('#search-input').value = '日本語'; $('#search-input').dispatchEvent(new Event('input', { bubbles: true }));
        await wait('Japanese search', () => !$('#search-next').disabled);
        assert('Japanese PDF search succeeds in WKWebView', true);
        $('#search-toggle').click();
      }
      if (total > 1) {
        let blankFrames = 0, frames = 0, watching = true, previousTurnFrame = null;
        const frameIntervals = [];
        const observe = timestamp => { if (!watching) return; frames++; if ($('#reading-area').dataset.turning === 'true') { if (previousTurnFrame !== null) frameIntervals.push(timestamp - previousTurnFrame); previousTurnFrame = timestamp; } else previousTurnFrame = null; const canvases = [...document.querySelectorAll('#pages canvas')]; if (!canvases.length || canvases.some(canvas => !canvas.width || !canvas.height)) blankFrames++; requestAnimationFrame(observe); };
        requestAnimationFrame(observe);
        const turnStarted = performance.now();
        $('#next-page').click();
        const pendingBookmarkDisabled = $('#bookmark-toggle').disabled;
        await settled(2);
        const nextPageMs = Math.round(performance.now() - turnStarted);
        assert(`bookmark waits for the visible page ${index + 1}`, pendingBookmarkDisabled);
        assert(`animated page is selectable ${index + 1}`, !$('#pages').inert);
        const final = Math.min(total, 6);
        for (let page = 1; page <= final; page++) navigate(page);
        await settled(final);
        watching = false;
        frameIntervals.sort((a, b) => a - b);
        const animationFrames = frameIntervals.length;
        const maxFrameMs = Math.round(frameIntervals.at(-1) || 0);
        const p95FrameMs = Math.round(frameIntervals[Math.floor(frameIntervals.length * .95)] || 0);
        observations[`document${index + 1}`] = { frames, blankFrames, pageCount: total, nextPageMs, animationFrames, maxFrameMs, p95FrameMs, renderedPages: document.querySelectorAll('#pages .pdf-page').length };
        if (index === 0) assert('fixture animation has no frame gap above 150 ms', animationFrames > 0 && maxFrameMs <= 150);
        assert(`page turn never removes the visible canvas ${index + 1}`, frames > 0 && blankFrames === 0);
        assert(`rapid page turns end on the last request ${index + 1}`, Number($('#page-input').value) === final);
        assert(`temporary page and animation resources are released ${index + 1}`, !$('.staging-pages') && !$('.paper-turn'));
        const originalMatchMedia = window.matchMedia;
        window.matchMedia = query => query === '(prefers-reduced-motion: reduce)' ? { matches: true } : originalMatchMedia.call(window, query);
        navigate(1); await settled(1);
        window.matchMedia = originalMatchMedia;
        assert(`reduced-motion page is not inert ${index + 1}`, !$('#pages').inert && !$('.paper-turn'));
        if (index === 0 || fixture.name.startsWith('oreilly-')) {
          const bodyPage = Math.max(1, Math.floor(total / 3));
          navigate(bodyPage); await settled(bodyPage);
          observations[`document${index + 1}`].bodyPage = bodyPage;
          assert(`body page renders in WKWebView ${index + 1}`, $('#pages canvas').width > 0);
        }
        if (index === 0 || (fixture.name.startsWith('oreilly-') && !observations.bookSpreadTested)) {
          navigate(1); await settled(1);
          select('#layout-select', 'spread'); await settled(1);
          assert(`spread displays two pages ${index + 1}`, document.querySelectorAll('#pages .pdf-page').length === 2);
          $('#next-page').click(); await settled(3);
          assert(`spread forward turn ends selectable ${index + 1}`, !$('#pages').inert && !$('.paper-turn'));
          $('#prev-page').click(); await settled(1);
          assert(`spread backward turn ends selectable ${index + 1}`, !$('#pages').inert && !$('.paper-turn'));
          select('#layout-select', 'single'); await settled(1);
          if (fixture.name.startsWith('oreilly-')) observations.bookSpreadTested = true;
        }
        navigate(1); await settled(1);
      }
      select('#theme-select', 'night'); assert(`night theme switches ${index + 1}`, document.body.dataset.theme === 'night'); select('#theme-select', 'paper');
      if (index === 0) {
        $('#bookmark-toggle').click();
        await wait('bookmark save', async () => (await window.desktop.getLibrary()).recent[0].state.bookmarks.includes(1));
        assert('bookmark persists through the Tauri bridge', true);
        const source = $('#pages canvas');
        const small = document.createElement('canvas'); small.width = 180; small.height = 120;
        small.getContext('2d').drawImage(source, 0, 0, source.width, source.height, 0, 0, 180, 120);
        await window.desktop.copyImage(small.toDataURL('image/png'));
        const [width, height] = await invoke('debug_clipboard_image_size');
        assert('native PNG clipboard receives rendered page pixels', width === 180 && height === 120);
      }
    }
    assert('no uncaught renderer errors', observations.errors.length === 0);
    await invoke('debug_smoke_report', { report: { passed: true, checks, observations }, snapshot });
  } catch (error) {
    await invoke('debug_smoke_report', { report: { passed: false, checks, error: `${error.message || error}\n${error.stack || ''}`, status: $('#reader-status')?.textContent, toast: $('#toast')?.textContent, observations }, snapshot }).catch(() => {});
  }
})();
