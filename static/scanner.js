// Real camera-based QR / barcode scanner.
//
// Strategy:
//   1. Prefer the browser's native BarcodeDetector when available
//      (Chrome, Edge, recent Safari). It's a few KB and has no extra
//      dependency. ~12 formats supported including QR, EAN-8/13,
//      UPC-A/E, Code128, Code39, Codabar, ITF, PDF417, DataMatrix.
//   2. Fall back to ZXing-Browser loaded from a CDN if BarcodeDetector
//      is missing. ZXing covers every common format and runs in WASM.
//
// The scanner is exposed as `window.RL.scanner.open()` so any view —
// the existing scanner kiosk, an "Add by scan" button, the item drawer
// — can trigger it without re-implementing camera logic.

(function () {
  const ZXING_URL =
    'https://unpkg.com/@zxing/browser@0.1.5/umd/zxing-browser.min.js';

  const state = {
    overlay: null,
    video: null,
    stream: null,
    raf: null,
    detector: null,
    zxing: null,
    decoder: null,
    onResult: null,
    busy: false,
    lastCode: null,
    lastAt: 0,
    statusEl: null,
    resultEl: null,
  };

  async function open(opts = {}) {
    if (state.overlay) return; // already open
    state.onResult = opts.onResult || defaultOnResult;
    buildOverlay();
    try {
      await startCamera();
      await startDecoder();
    } catch (e) {
      setStatus(`Scanner unavailable: ${e.message || e}`, 'err');
      console.warn('[RL.scanner]', e);
    }
  }

  function close() {
    cancelAnimationFrame(state.raf);
    state.raf = null;
    if (state.decoder && state.decoder.reset) {
      try { state.decoder.reset(); } catch (_) {}
    }
    state.decoder = null;
    if (state.stream) {
      state.stream.getTracks().forEach((t) => t.stop());
      state.stream = null;
    }
    if (state.overlay) {
      state.overlay.remove();
      state.overlay = null;
    }
    state.video = null;
    state.statusEl = null;
    state.resultEl = null;
    state.lastCode = null;
    state.busy = false;
  }

  function buildOverlay() {
    const root = document.createElement('div');
    root.className = 'rl-scanner-overlay';
    Object.assign(root.style, {
      position: 'fixed', inset: '0', zIndex: '9999',
      background: 'rgba(0,0,0,0.85)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexDirection: 'column', gap: '12px', padding: '20px',
      fontFamily: 'Inter, system-ui, sans-serif', color: '#e5e7eb',
    });

    const card = document.createElement('div');
    Object.assign(card.style, {
      background: '#0f172a', border: '1px solid #334155',
      borderRadius: '8px', padding: '12px', maxWidth: '560px',
      width: '100%', boxShadow: '0 20px 60px rgba(0,0,0,0.6)',
    });

    const header = document.createElement('div');
    header.innerHTML =
      `<div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">
        <strong style="font-size:14px;letter-spacing:0.04em;">SCAN</strong>
        <span style="flex:1"></span>
        <button class="rl-scan-close"
          style="background:transparent;border:1px solid #475569;color:#e5e7eb;
                 padding:4px 10px;border-radius:4px;cursor:pointer;font-size:12px;">
          Esc · Close
        </button>
      </div>`;
    header.querySelector('.rl-scan-close').addEventListener('click', close);

    const video = document.createElement('video');
    video.setAttribute('playsinline', 'true');
    video.muted = true;
    Object.assign(video.style, {
      width: '100%', maxHeight: '360px', borderRadius: '4px',
      background: '#000', objectFit: 'cover',
    });

    const status = document.createElement('div');
    Object.assign(status.style, {
      marginTop: '8px', fontSize: '12px', fontFamily:
      'JetBrains Mono, ui-monospace, monospace', color: '#94a3b8',
    });
    status.textContent = 'Requesting camera…';

    const result = document.createElement('div');
    Object.assign(result.style, {
      marginTop: '6px', fontSize: '12px', fontFamily:
      'JetBrains Mono, ui-monospace, monospace', color: '#fbbf24',
      minHeight: '14px',
    });

    card.appendChild(header);
    card.appendChild(video);
    card.appendChild(status);
    card.appendChild(result);
    root.appendChild(card);
    document.body.appendChild(root);

    root.addEventListener('click', (e) => { if (e.target === root) close(); });
    document.addEventListener('keydown', escListener, true);

    state.overlay = root;
    state.video = video;
    state.statusEl = status;
    state.resultEl = result;
  }

  function escListener(e) {
    if (e.key === 'Escape' && state.overlay) {
      close();
      document.removeEventListener('keydown', escListener, true);
    }
  }

  function setStatus(msg, kind) {
    if (!state.statusEl) return;
    state.statusEl.textContent = msg;
    state.statusEl.style.color =
      kind === 'err' ? '#f87171' : kind === 'ok' ? '#34d399' : '#94a3b8';
  }

  async function startCamera() {
    if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
      throw new Error('camera API not available');
    }
    setStatus('Requesting camera…');
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        facingMode: { ideal: 'environment' },
        width:  { ideal: 1280 },
        height: { ideal: 720 },
      },
    });
    state.stream = stream;
    state.video.srcObject = stream;
    await new Promise((res) => {
      state.video.onloadedmetadata = () => { state.video.play().then(res, res); };
    });
    setStatus('Aim at a barcode or QR code…');
  }

  async function startDecoder() {
    if ('BarcodeDetector' in window) {
      try {
        const supported = await window.BarcodeDetector.getSupportedFormats();
        state.detector = new window.BarcodeDetector({ formats: supported });
        loopNative();
        return;
      } catch (e) {
        console.info('[RL.scanner] BarcodeDetector unavailable, using ZXing', e);
      }
    }
    await ensureZxing();
    const Z = window.ZXingBrowser;
    if (!Z) throw new Error('ZXing failed to load');
    state.decoder = new Z.BrowserMultiFormatReader();
    state.decoder.decodeFromVideoElement(state.video, (result, err) => {
      if (result) handleResult(result.getText());
    });
  }

  function loopNative() {
    if (!state.detector || !state.video) return;
    const tick = async () => {
      if (!state.detector || !state.video) return;
      try {
        if (state.video.readyState >= 2 && !state.busy) {
          state.busy = true;
          const codes = await state.detector.detect(state.video);
          state.busy = false;
          if (codes && codes[0]) handleResult(codes[0].rawValue);
        }
      } catch (_) { state.busy = false; }
      state.raf = requestAnimationFrame(tick);
    };
    state.raf = requestAnimationFrame(tick);
  }

  function ensureZxing() {
    if (window.ZXingBrowser) return Promise.resolve();
    return new Promise((res, rej) => {
      const s = document.createElement('script');
      s.src = ZXING_URL;
      s.crossOrigin = 'anonymous';
      s.onload  = res;
      s.onerror = () => rej(new Error('failed to load ZXing'));
      document.head.appendChild(s);
    });
  }

  // Debounce repeats — if the camera is sitting on a label, a single
  // detection stream can fire dozens of times per second.
  function handleResult(code) {
    if (!code) return;
    const now = Date.now();
    if (code === state.lastCode && now - state.lastAt < 1500) return;
    state.lastCode = code;
    state.lastAt = now;
    if (state.resultEl) state.resultEl.textContent = `→ ${code}`;
    setStatus('Decoded — looking up…', 'ok');
    Promise.resolve(state.onResult(code)).catch((e) => {
      setStatus(`Lookup failed: ${e.message || e}`, 'err');
    });
  }

  async function defaultOnResult(code) {
    let result = null;
    try {
      if (window.RL && window.RL.api) {
        result = await window.RL.api.get(`/api/lookup/${encodeURIComponent(code)}`);
      }
    } catch (e) {
      // Network errors shouldn't keep the camera frozen.
    }

    // 1. Local hit — open the existing item drawer if one exists.
    if (result && result.local_item_id && window.__inv && window.__inv.openItem) {
      window.__inv.openItem(result.local_item_id);
      close();
      toast(`Found · ${result.local_item_id}`, 'OK');
      return;
    }
    // 2. External hit — show a small import card.
    if (result && result.external && result.external.length > 0) {
      showImportCard(code, result.external[0]);
      return;
    }
    // 3. No hit — just surface the raw code so the user can do something.
    setStatus(`No match for ${code}. Tap a button below.`, 'err');
    showImportCard(code, null);
  }

  function showImportCard(barcode, hit) {
    if (!state.resultEl) return;
    const card = document.createElement('div');
    Object.assign(card.style, {
      marginTop: '10px', padding: '10px',
      background: '#1e293b', border: '1px solid #475569',
      borderRadius: '6px', fontSize: '12px', color: '#e5e7eb',
      lineHeight: '1.5',
    });
    const name = (hit && hit.name) || '(unknown product)';
    const brand = (hit && hit.brand) || '';
    const source = (hit && hit.source) || '—';
    card.innerHTML = `
      <div><b>Barcode</b> · <code>${escapeHtml(barcode)}</code></div>
      <div><b>Match</b> · ${escapeHtml(name)}${brand ? ' · ' + escapeHtml(brand) : ''} <span style="color:#64748b;">[${escapeHtml(source)}]</span></div>
      <div style="margin-top:8px;display:flex;gap:6px;flex-wrap:wrap;">
        <button class="rl-act rl-import"
          style="background:#0ea5e9;color:#000;border:0;padding:6px 12px;border-radius:4px;cursor:pointer;">Create item</button>
        <button class="rl-act rl-copy"
          style="background:transparent;color:#e5e7eb;border:1px solid #475569;padding:6px 12px;border-radius:4px;cursor:pointer;">Copy code</button>
        <button class="rl-act rl-resume"
          style="background:transparent;color:#e5e7eb;border:1px solid #475569;padding:6px 12px;border-radius:4px;cursor:pointer;">Scan another</button>
      </div>`;
    state.resultEl.innerHTML = '';
    state.resultEl.appendChild(card);

    card.querySelector('.rl-copy').addEventListener('click', async () => {
      try { await navigator.clipboard.writeText(barcode); toast('Copied'); }
      catch (_) { toast('Copy failed', 'ERR', 'warn'); }
    });
    card.querySelector('.rl-resume').addEventListener('click', () => {
      state.resultEl.innerHTML = '';
      state.lastCode = null;
      setStatus('Aim at a barcode or QR code…');
    });
    card.querySelector('.rl-import').addEventListener('click', async () => {
      try {
        const sku = (hit && hit.mpn) || `BC-${barcode}`;
        const payload = {
          sku,
          name: name === '(unknown product)' ? sku : name,
          cat: (hit && hit.category) || 'Spare parts',
          brand: brand || null,
          barcode,
          // Don't fabricate a $0 cost when the lookup didn't return
          // one — that silently misleads inventory valuations.
          cost: (hit && typeof hit.price === 'number') ? hit.price : null,
          price: (hit && typeof hit.price === 'number') ? hit.price : null,
          unit: 'ea',
          min: 0,
          max: 0,
          qty: 0,
          allocated: 0,
          tags: [],
          loc: [],
          img: null,
        };
        const created = await window.RL.api.post('/api/items', payload);
        toast(`Created ${created.id} · reloading…`);
        close();
        // The React views aren't reactive to window.ITEMS changes —
        // they read the global once at mount and never re-read it.
        // Reload is the simplest reliable way to surface the new
        // item in every view (catalog, dashboard counts, drawer).
        // Drop a marker so the page can re-open the drawer if we
        // teach app.jsx to read it (cheap, fail-safe if it doesn't).
        try { sessionStorage.setItem('racklog.openAfterReload', created.id); } catch (_) {}
        setTimeout(() => location.reload(), 250);
      } catch (e) {
        toast(`Import failed: ${e.message || e}`, 'ERR', 'warn');
      }
    });
  }

  const escapeHtml = (s) => window.RL.util.escapeHtml(s);

  function toast(msg, tag, variant) {
    if (window.__inv && window.__inv.toast) {
      window.__inv.toast(msg, { tag: tag || 'OK', variant: variant || 'pos' });
    } else {
      console.log(`[scan] ${msg}`);
    }
  }

  // Global keyboard shortcut: 'B' opens the real scanner. ('S' already
  // opens the prototype's mock scanner — both stay so users can decide.)
  document.addEventListener('keydown', (e) => {
    const inField = ['INPUT', 'TEXTAREA'].includes((e.target || {}).tagName);
    if (inField) return;
    if (e.key === 'b' || e.key === 'B') {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      e.preventDefault();
      open();
    }
  });

  window.RL = window.RL || {};
  window.RL.scanner = { open, close };
})();
