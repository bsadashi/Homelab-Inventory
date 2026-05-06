// Floating ops bar: surfaces backend health and "data is stale" hints
// outside the React tree so we never have to touch the prototype's view
// components. Listens for events emitted by static/api.js and renders a
// single fixed-position element appended to <body>.
//
// What it shows:
//   ● green/amber/red dot     — connection state from /api/activity/verify polls
//   ● "v2" / "—"             — the audit chain head version, if any
//   ● "Refresh" button        — appears only when the chain head has
//                               advanced since this page loaded; one
//                               click triggers location.reload(), which
//                               is the simplest way to re-hydrate the
//                               (non-reactive) view tree.

(function () {
  if (!window.RL || !window.RL.api) {
    console.warn('[RL.opsbar] api.js not loaded; ops bar disabled');
    return;
  }
  if (document.getElementById('rl-ops-bar')) return; // already mounted

  const root = document.createElement('div');
  root.id = 'rl-ops-bar';
  Object.assign(root.style, {
    position: 'fixed',
    bottom: '12px',
    right: '12px',
    zIndex: '9998',
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '6px 10px',
    background: 'rgba(15, 23, 42, 0.92)',
    color: '#e5e7eb',
    border: '1px solid #334155',
    borderRadius: '999px',
    fontSize: '11px',
    fontFamily: 'JetBrains Mono, ui-monospace, monospace',
    letterSpacing: '0.04em',
    boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
    pointerEvents: 'auto',
  });

  const dot = document.createElement('span');
  Object.assign(dot.style, {
    display: 'inline-block', width: '8px', height: '8px',
    borderRadius: '50%', background: '#94a3b8',
  });
  const label = document.createElement('span');
  label.textContent = 'API · …';
  const refreshBtn = document.createElement('button');
  refreshBtn.textContent = '↻ data updated';
  Object.assign(refreshBtn.style, {
    display: 'none', padding: '3px 8px',
    background: '#f59e0b', color: '#0f172a',
    border: '0', borderRadius: '4px',
    fontSize: '11px', fontWeight: '600', cursor: 'pointer',
  });
  refreshBtn.title = 'New data on the server. Click to reload.';
  refreshBtn.addEventListener('click', () => location.reload());

  root.appendChild(dot);
  root.appendChild(label);
  root.appendChild(refreshBtn);
  document.body.appendChild(root);

  // Connection / chain status updates arrive from RL.api's polling.
  RL.api.on('verify', (v) => {
    if (v && v.valid) {
      dot.style.background = '#22c55e';
      label.textContent = `API · v${v.version || '?'} · ok`;
    } else {
      dot.style.background = '#f87171';
      label.textContent = `API · v${(v && v.version) || '?'} · CHAIN BROKEN`;
    }
  });
  RL.api.on('online', (ok) => {
    if (!ok) {
      dot.style.background = '#f87171';
      label.textContent = 'API · offline';
    }
  });
  RL.api.on('stale', () => {
    refreshBtn.style.display = 'inline-block';
  });
  RL.api.on('unauthorized', () => {
    dot.style.background = '#fbbf24';
    label.textContent = 'API · 401 · token required';
  });
})();
