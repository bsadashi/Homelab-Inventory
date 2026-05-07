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

  // ── identity (logged-in user + role + logout) ─────────────────────
  const userPill = document.createElement('span');
  Object.assign(userPill.style, {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '6px',
    padding: '2px 8px',
    border: '1px solid #475569',
    borderRadius: '4px',
    color: '#e5e7eb',
    fontSize: '11px',
  });
  userPill.style.display = 'none';
  userPill.title = 'Logged-in user';

  const logoutBtn = document.createElement('button');
  logoutBtn.textContent = '⎋';
  logoutBtn.title = 'Log out';
  Object.assign(logoutBtn.style, {
    background: 'transparent', color: '#e5e7eb', border: '1px solid #475569',
    padding: '2px 8px', borderRadius: '4px', cursor: 'pointer', fontSize: '11px',
    display: 'none',
  });
  logoutBtn.addEventListener('click', async () => {
    try {
      await fetch('/api/auth/logout', { method: 'POST', credentials: 'same-origin' });
    } catch (_) {}
    document.location.replace('/login');
  });

  // /api/auth/me populates the identity pill once the page loads.
  (async function loadIdentity() {
    try {
      const r = await fetch('/api/auth/me', { credentials: 'same-origin' });
      if (!r.ok) return;
      const me = await r.json();
      const u = me.user;
      const roleColors = {
        admin:    '#f59e0b',
        operator: '#10b981',
        viewer:   '#94a3b8',
      };
      userPill.innerHTML = '';
      const dotEl = document.createElement('span');
      Object.assign(dotEl.style, {
        width: '6px', height: '6px', borderRadius: '50%',
        background: roleColors[u.role] || '#94a3b8',
      });
      const nameEl = document.createElement('span');
      nameEl.textContent = u.username;
      const roleEl = document.createElement('span');
      roleEl.textContent = `· ${u.role}`;
      roleEl.style.color = '#94a3b8';
      userPill.appendChild(dotEl);
      userPill.appendChild(nameEl);
      userPill.appendChild(roleEl);
      userPill.style.display = 'inline-flex';
      // Hide the API token button when the user is on a real
      // session — it's an API-only escape hatch and confuses the
      // dashboard UX otherwise.
      if (me.source === 'session') {
        logoutBtn.style.display = 'inline-block';
        // tokenBtn declared below; defer hiding to next tick.
        setTimeout(() => {
          const tb = document.querySelector('#rl-ops-bar [data-rl-tokenbtn]');
          if (tb) tb.style.display = 'none';
        }, 0);
      } else if (me.source === 'apikey' || me.source === 'bootstrap' || me.source === 'trustedheader') {
        // Bootstrap or token: don't expose logout; the user has no
        // session to revoke.
      }
    } catch (_) {}
  })();

  // ── auth token entry (F3) ────────────────────────────────────────
  // Small key-icon button that pops a prompt for the bearer token.
  // Stored value lives in localStorage('racklog.token') via RL.api.
  const tokenBtn = document.createElement('button');
  tokenBtn.textContent = '🔑';
  tokenBtn.title = 'Set / clear API bearer token';
  tokenBtn.setAttribute('data-rl-tokenbtn', '1');
  Object.assign(tokenBtn.style, {
    background: 'transparent', color: '#e5e7eb', border: '1px solid #475569',
    padding: '2px 8px', borderRadius: '4px', cursor: 'pointer', fontSize: '11px',
  });
  tokenBtn.addEventListener('click', () => {
    const current = RL.api.getToken();
    const next = window.prompt(
      'API bearer token (leave blank to clear):',
      current || ''
    );
    if (next === null) return; // user pressed cancel
    RL.api.setToken(next.trim());
    RL.api.refreshNow();
  });

  // ── CSV export / import (F6) ─────────────────────────────────────
  const exportBtn = document.createElement('button');
  exportBtn.textContent = '⬇ csv';
  exportBtn.title = 'Download all items as CSV';
  Object.assign(exportBtn.style, {
    background: 'transparent', color: '#e5e7eb', border: '1px solid #475569',
    padding: '2px 8px', borderRadius: '4px', cursor: 'pointer', fontSize: '11px',
  });
  exportBtn.addEventListener('click', async () => {
    try {
      await RL.api.download('/api/exports/items.csv', 'items.csv');
    } catch (e) {
      alert(`Export failed: ${e.message || e}`);
    }
  });

  const importBtn = document.createElement('button');
  importBtn.textContent = '⬆ csv';
  importBtn.title = 'Bulk-import items from a CSV file';
  Object.assign(importBtn.style, {
    background: 'transparent', color: '#e5e7eb', border: '1px solid #475569',
    padding: '2px 8px', borderRadius: '4px', cursor: 'pointer', fontSize: '11px',
  });
  const fileInput = document.createElement('input');
  fileInput.type = 'file';
  fileInput.accept = '.csv,text/csv';
  fileInput.style.display = 'none';
  importBtn.addEventListener('click', () => fileInput.click());
  fileInput.addEventListener('change', async () => {
    const file = fileInput.files && fileInput.files[0];
    if (!file) return;
    try {
      const text = await file.text();
      const headers = { 'Content-Type': 'text/csv' };
      const tok = RL.api.getToken();
      if (tok) headers['Authorization'] = `Bearer ${tok}`;
      const r = await fetch('/api/exports/items.csv', {
        method: 'POST', headers, body: text, credentials: 'same-origin',
      });
      const result = await r.json().catch(() => ({}));
      if (!r.ok) throw new Error(result.message || `${r.status} ${r.statusText}`);
      const ok = result.created + result.updated;
      const errs = (result.errors || []).length;
      alert(
        `Import complete · ${result.created} created · ${result.updated} updated`
        + (errs ? ` · ${errs} row error(s) — see response for details` : '')
        + (ok > 0 ? '\n\nReloading to refresh views…' : '')
      );
      if (ok > 0) location.reload();
    } catch (e) {
      alert(`Import failed: ${e.message || e}`);
    } finally {
      fileInput.value = '';
    }
  });

  root.appendChild(dot);
  root.appendChild(label);
  root.appendChild(userPill);
  root.appendChild(tokenBtn);
  root.appendChild(exportBtn);
  root.appendChild(importBtn);
  root.appendChild(fileInput);
  root.appendChild(refreshBtn);
  root.appendChild(logoutBtn);
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
