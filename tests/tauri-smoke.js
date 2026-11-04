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
        await wait('Japanese clipboard', async () => { const text = await invoke('debug_clipboard_text').catch(() => ''); observations.copyBusy = $('#copy-page').dataset.busy; return text.includes('日本語の文章を読む'); });
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
        const p50FrameMs = Math.round(frameIntervals[Math.floor(frameIntervals.length * .5)] || 0);
        observations[`document${index + 1}`] = { frames, blankFrames, pageCount: total, nextPageMs, animationFrames, maxFrameMs, p95FrameMs, p50FrameMs, renderedPages: document.querySelectorAll('#pages .pdf-page').length };
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
      const pageRect = $('#pages .pdf-page').getBoundingClientRect();
      const readerRect = $('#reader').getBoundingClientRect();
      assert(`entire page fits without reader scrolling ${index + 1}`, pageRect.left >= readerRect.left - 1 && pageRect.right <= readerRect.right + 1 && pageRect.top >= readerRect.top - 1 && pageRect.bottom <= readerRect.bottom + 1 && getComputedStyle($('#reader')).overflow === 'hidden');
      if (index === 0) {
        const dimensions = () => { const rect = $('#pages .pdf-page').getBoundingClientRect(); return [rect.width, rect.height]; };
        const beforePanels = dimensions();
        $('#toggle-sidebar').click(); await sleep(100);
        assert('sidebar overlays do not resize the page', dimensions().every((value, i) => Math.abs(value - beforePanels[i]) < 1));
        $('#toggle-sidebar').click();
        $('#tools-toggle').click(); await sleep(100);
        assert('reading tools do not resize the page', dimensions().every((value, i) => Math.abs(value - beforePanels[i]) < 1));
        $('#tools-toggle').click();
        $('#magnify-toggle').click();
        await wait('magnifier renders crisp detail', () => !$('#magnifier').hidden && !$('#magnifier-copy').disabled);
        const sourceSize = dimensions();
        $('#magnifier-plus').click(); await wait('magnifier zoom settles', () => !$('#magnifier-copy').disabled);
        assert('temporary magnification preserves the full-page view', dimensions().every((value, i) => Math.abs(value - sourceSize[i]) < 1));
        const lens = $('#magnifier-canvas');
        const lensRect = lens.getBoundingClientRect();
        const oldCapture = lens.setPointerCapture;
        lens.setPointerCapture = () => {};
        const markerBefore = $('.magnifier-source').style.left;
        lens.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 41, button: 0, clientX: lensRect.left + 150, clientY: lensRect.top + 120 }));
        lens.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 41, button: 0, clientX: lensRect.left + 210, clientY: lensRect.top + 140 }));
        lens.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 41, button: 0, clientX: lensRect.left + 210, clientY: lensRect.top + 140 }));
        lens.setPointerCapture = oldCapture;
        await wait('magnifier drag sharpens', () => !$('#magnifier-copy').disabled);
        assert('dragging the magnifier moves the inspected region', $('.magnifier-source').style.left !== markerBefore && !lens.dataset.dragging);
        const copySize = [lens.width, lens.height]; $('#magnifier-copy').click();
        await wait('magnifier native image copy', async () => { const size = await invoke('debug_clipboard_image_size').catch(() => null); return size?.length === 2 && size.every((value, i) => value === copySize[i]); });
        assert('magnified detail copies to the native clipboard', true);
        $('#magnifier-close').click();
        assert('closing the magnifier restores the unobstructed page', $('#magnifier').hidden && !$('.magnifier-source'));
        const reader = $('#reader');
        const captureMethods = [reader.setPointerCapture, reader.hasPointerCapture, reader.releasePointerCapture];
        let heldPointer = false;
        reader.setPointerCapture = () => { heldPointer = true; };
        reader.hasPointerCapture = () => heldPointer;
        reader.releasePointerCapture = () => { heldPointer = false; };
        const gesture = async (progress, behavior = 'normal') => {
          const corner = $('.page-corner.right'); const rect = corner.closest('.pdf-page').getBoundingClientRect();
          const x = rect.right - 12, y = rect.bottom - 12;
          const dispatch = (target, type, atX = x) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 77, button: 0, clientX: atX, clientY: y }));
          dispatch(corner, 'pointerdown');
          if (behavior === 'early-cancel') { dispatch(reader, 'pointercancel'); return; }
          await wait('paper held at corner', () => $('#reading-area').dataset.turnPhase === 'dragging');
          dispatch(reader, 'pointermove', x - rect.width * progress);
          if (behavior === 'escape') { reader.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' })); return; }
          if (behavior === 'return') {
            await sleep(100); dispatch(reader, 'pointermove', x); await sleep(100); dispatch(reader, 'pointerup'); return;
          }
          await sleep(behavior === 'hold' ? 240 : 100);
          if (behavior !== 'hold') dispatch(reader, 'pointermove', x - rect.width * progress);
          dispatch(reader, 'pointerup', x - rect.width * progress);
        };
        try {
          await gesture(0, 'early-cancel'); await settled(1);
          assert('cancel before paper preparation keeps the original page', !$('.staging-pages') && !$('#reader').dataset.grabbing);
          await gesture(.12); await settled(1);
          assert('returning the held corner cancels the turn', !$('#pages').inert && !$('.paper-turn'));
          await gesture(.6, 'return'); await settled(1);
          assert('dragging out and back to the origin cancels', !$('#pages').inert && !$('#reader').dataset.grabbing);
          await gesture(.1, 'hold'); await settled(1);
          assert('a held short drag does not retain flick velocity', !$('#pages').inert && !$('#reader').dataset.grabbing);
          await gesture(.6, 'escape'); await settled(1);
          assert('Escape cancels a held paper turn', !$('#pages').inert && !$('#reader').dataset.grabbing);
          await gesture(.6); await settled(2);
          assert('dragging the corner across commits the turn', !$('#pages').inert && !$('.paper-turn'));
          navigate(1); await settled(1);
        } finally { [reader.setPointerCapture, reader.hasPointerCapture, reader.releasePointerCapture] = captureMethods; }
      }
      if (index === 0 || (fixture.name.startsWith('oreilly-') && !observations.bookPageMapTested)) {
        navigate(1); await settled(1);
        $('#page-map-toggle').click();
        await wait('page map opens', () => $('#page-map')?.open);
        const target = Math.max(2, Math.floor(total * .72));
        $('#page-map-input').value = String(target);
        $('#page-map-input').dispatchEvent(new Event('input', { bubbles: true }));
        await wait('typed target preview', () => $('#page-map-preview canvas').width > 0 && $('#page-map-preview').dataset.loading === 'false' && $(`.page-map-thumb[data-page="${target}"]`)?.dataset.selected === 'true');
        assert(`arbitrary page preview is immediate and bounded ${index + 1}`, document.querySelectorAll('.page-map-thumb').length <= 36 && Number($('#page-map-input').value) === target);
        $('#page-map-form').requestSubmit(); await settled(target);
        assert(`typed page jump reaches its target ${index + 1}`, !$('#page-map').open);
        $('#page-map-toggle').click(); await wait('page map reopens', () => $('#page-map').open);
        assert(`jump return is available ${index + 1}`, !$('#page-map-back').hidden);
        $('#page-map-back').click(); await settled(1);
        $('#page-map-toggle').click(); await wait('page map opens for scrub', () => $('#page-map').open);
        $('#page-map-range').value = String(total); $('#page-map-range').dispatchEvent(new Event('input', { bubbles: true }));
        await wait('last page thumbnail', () => $(`.page-map-thumb[data-page="${total}"]`)?.dataset.ready === 'true');
        $(`.page-map-thumb[data-page="${total}"]`).click(); await settled(total);
        assert(`scrub and thumbnail reach the last page ${index + 1}`, !$('#page-map').open);
        navigate(1); await settled(1);
        if (index === 0) {
          await invoke('debug_resize_window', { width: 900, height: 650 }); await sleep(250); await settled(1);
          $('#page-map-toggle').click(); await wait('compact page map opens', () => $('#page-map').open);
          $('#page-map-input').value = String(total); $('#page-map-input').dispatchEvent(new Event('input', { bubbles: true }));
          await wait('compact preview renders', () => $('#page-map-preview').dataset.loading === 'false');
          const modal = $('#page-map').getBoundingClientRect();
          assert('page map fits a compact native window', modal.left >= 0 && modal.right <= innerWidth && modal.top >= 0 && modal.bottom <= innerHeight && $('#page-map-scroll').clientWidth > 250 && $('#page-map-input').getBoundingClientRect().right < $('#page-map-scroll').getBoundingClientRect().left);
          $('#page-map-form').requestSubmit(); await settled(total);
          assert('compact page map reaches its typed destination', !$('#page-map').open);
          await invoke('debug_resize_window', { width: 1380, height: 940 }); await sleep(250); navigate(1); await settled(1);
        }
        if (fixture.name.startsWith('oreilly-')) observations.bookPageMapTested = true;
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
