// RACKLOG API client. Lives on window.RL so any view can poke it.
//
// Loaded BEFORE data.jsx so the bootstrap snapshot from the server is
// already on the page when data.jsx wires it onto window globals. The
// client itself does not block initial render — it backs the floating
// ops bar, the CSV exports, and any future mutation flows.

(function () {
  const TOKEN_KEY = 'racklog.token';
  const POLL_KEY  = 'racklog.poll';
  const POLL_DEFAULT_MS = 30000;
  const POLL_MIN_MS = 5000;

  const listeners = {};
  const on  = (k, fn) => { (listeners[k] = listeners[k] || []).push(fn); };
  const off = (k, fn) => { listeners[k] = (listeners[k] || []).filter(x => x !== fn); };
  const emit = (k, payload) => (listeners[k] || []).forEach(fn => {
    try { fn(payload); } catch (e) { /* listener fault must not stop the loop */ }
  });

  function getToken() {
    try { return localStorage.getItem(TOKEN_KEY) || ''; } catch (e) { return ''; }
  }
  function setToken(v) {
    try {
      if (v) localStorage.setItem(TOKEN_KEY, v);
      else   localStorage.removeItem(TOKEN_KEY);
    } catch (e) { /* private mode — best-effort */ }
    emit('token', v);
  }

  async function call(method, path, body) {
    const headers = { 'Accept': 'application/json' };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    const tok = getToken();
    if (tok) headers['Authorization'] = `Bearer ${tok}`;
    const init = { method, headers, credentials: 'same-origin' };
    if (body !== undefined) init.body = JSON.stringify(body);
    const r = await fetch(path, init);
    if (r.status === 401) {
      emit('unauthorized', { path });
      // Cookie-based auth: bounce to /login so the user can re-auth
      // without losing context. Only do this for navigations
      // through the dashboard (not for token-only API callers).
      if (!tok && document.location.pathname !== '/login') {
        document.location.replace('/login');
      }
    }
    if (!r.ok) {
      let msg = `${r.status} ${r.statusText}`;
      try { const j = await r.json(); if (j && j.message) msg = j.message; } catch (_) {}
      const err = new Error(msg);
      err.status = r.status;
      err.path = path;
      throw err;
    }
    if (r.status === 204) return null;
    const ct = r.headers.get('content-type') || '';
    if (ct.includes('application/json')) return r.json();
    return r.text();
  }

  const get  = (p)        => call('GET', p);
  const post = (p, body)  => call('POST', p, body);
  const put  = (p, body)  => call('PUT', p, body);
  const del  = (p)        => call('DELETE', p);

  // CSV downloads via an off-screen anchor so we get a real "Save As"
  // dialog and the response carries the proper Content-Disposition.
  async function download(path, filename) {
    const tok = getToken();
    const headers = {};
    if (tok) headers['Authorization'] = `Bearer ${tok}`;
    const r = await fetch(path, { headers, credentials: 'same-origin' });
    if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
    const blob = await r.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => { URL.revokeObjectURL(url); a.remove(); }, 100);
  }

  // ---- chain + connection probes ---------------------------------------

  let lastVerify = null;
  let online = navigator.onLine !== false;
  window.addEventListener('online',  () => { online = true;  emit('online', true); });
  window.addEventListener('offline', () => { online = false; emit('online', false); });

  // The audit chain's head hash advances exactly once per mutation, so
  // we treat it as the single source of truth for "has anything changed
  // upstream since this page loaded?" — no extra endpoints required.
  const initialHead =
    (window.__RACKLOG_BOOTSTRAP__ &&
     window.__RACKLOG_BOOTSTRAP__.activity &&
     window.__RACKLOG_BOOTSTRAP__.activity[0] &&
     window.__RACKLOG_BOOTSTRAP__.activity[0].hash) || null;
  let pageHead = initialHead;
  let staleSince = null;

  async function probe() {
    try {
      const v = await get('/api/activity/verify');
      lastVerify = v;
      online = true;
      emit('verify', v);
      emit('online', true);
      if (v && v.head && pageHead && v.head !== pageHead && !staleSince) {
        staleSince = Date.now();
        emit('stale', { previous: pageHead, current: v.head });
      }
      if (v && v.head && !pageHead) pageHead = v.head;
      return v;
    } catch (e) {
      online = false;
      emit('online', false);
      throw e;
    }
  }

  // ---- polling ---------------------------------------------------------

  let pollMs = (() => {
    try { return parseInt(localStorage.getItem(POLL_KEY)) || POLL_DEFAULT_MS; }
    catch (e) { return POLL_DEFAULT_MS; }
  })();
  let pollTimer = null;
  let pollPaused = false;

  function setInterval_(ms) {
    pollMs = Math.max(POLL_MIN_MS, ms | 0);
    try { localStorage.setItem(POLL_KEY, String(pollMs)); } catch (e) {}
    schedule();
  }
  function pause(p)  { pollPaused = !!p; schedule(); }
  function schedule() {
    if (pollTimer) clearTimeout(pollTimer);
    if (pollPaused) return;
    pollTimer = setTimeout(tick, pollMs);
  }
  async function tick() {
    try { await probe(); } catch (_) {}
    schedule();
  }

  // Kick off after the page has mounted so the initial chain probe shows
  // up in the ops bar without delaying first paint.
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => { tick(); });
  } else {
    setTimeout(tick, 250);
  }

  window.RL = window.RL || {};
  window.RL.api = {
    get, post, put, delete: del,
    download,
    getToken, setToken,
    probe,
    on, off,
    state: () => ({
      online,
      verify: lastVerify,
      pageHead,
      stale: !!staleSince,
      staleSince,
      pollMs,
      pollPaused,
      hasToken: !!getToken(),
    }),
    setPollMs: setInterval_,
    pausePolling: pause,
    refreshNow: tick,
    reload: () => location.reload(),
  };
})();
