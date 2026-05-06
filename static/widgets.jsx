// Shared widgets used across views.
const { useState, useEffect, useMemo, useRef, useCallback } = React;

// ─── Sparkline ─────────────────────────────────────────────
function Sparkline({ data, width = 100, height = 28, fill = false, accent = 'var(--accent)' }) {
  if (!data || !data.length) return null;
  const min = Math.min(...data), max = Math.max(...data);
  const range = max - min || 1;
  const step = width / (data.length - 1);
  const pts = data.map((v, i) => [i * step, height - ((v - min) / range) * (height - 4) - 2]);
  const d = pts.map((p, i) => (i === 0 ? `M${p[0]},${p[1]}` : `L${p[0]},${p[1]}`)).join(' ');
  const dFill = `${d} L${width},${height} L0,${height} Z`;
  return (
    <svg className={`spark ${fill ? 'fill' : ''}`} width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      {fill && <path d={dFill} />}
      <path d={d} style={{ stroke: accent }} />
    </svg>
  );
}

// ─── Pill / Badge ──────────────────────────────────────────
function Pill({ children, variant, dot = false }) {
  return <span className={`pill ${variant || ''} ${dot ? 'dot' : ''}`}>{children}</span>;
}

// ─── Status pill for orders / etc ──────────────────────────
const STATUS_VARIANT = {
  draft: '', ordered: 'info', 'in-transit': 'info', received: 'pos',
  cancelled: 'neg', open: 'warn', picking: 'info', shipped: 'pos',
  done: 'pos', pending: 'warn', scheduled: 'info',
};
function StatusPill({ status }) {
  return <Pill variant={STATUS_VARIANT[status] || ''} dot>{status}</Pill>;
}

// ─── Bar cell (qty vs max) ─────────────────────────────────
function BarCell({ value, min, max }) {
  const pct = Math.max(0, Math.min(100, (value / Math.max(max || 1, 1)) * 100));
  const variant = value === 0 ? 'neg' : value < min ? 'warn' : '';
  return (
    <div className="bar-cell">
      <div className={`bar ${variant}`} style={{ minWidth: 40 }}><span style={{ width: `${pct}%` }} /></div>
    </div>
  );
}

// ─── Ticker bar ─────────────────────────────────────────────
function Ticker() {
  const items = [
    { lbl: 'SOH', val: STATUS.totalUnits, cls: '' },
    { lbl: 'SKUS', val: STATUS.totalSKUs, cls: '' },
    { lbl: 'VAL', val: '$' + STATUS.totalValue.toLocaleString(), cls: 'pos' },
    { lbl: 'LOW', val: STATUS.lowStock, cls: 'warn' },
    { lbl: 'OOS', val: STATUS.outOfStock, cls: 'neg' },
    { lbl: 'OPEN-PO', val: STATUS.openPOs, cls: '' },
    { lbl: 'OPEN-SO', val: STATUS.openSOs, cls: '' },
    { lbl: 'XFR', val: STATUS.pendingTransfers, cls: 'warn' },
    { lbl: 'CYCL', val: STATUS.scheduledCounts, cls: '' },
    { lbl: 'SERIAL', val: STATUS.serializedUnits, cls: '' },
    { lbl: 'CPU·LAB', val: '23%', cls: 'pos' },
    { lbl: 'TEMP·RACK-A', val: '24.1°C', cls: 'pos' },
    { lbl: 'PWR-DRAW', val: '412W', cls: '' },
    { lbl: 'NET-IN', val: '142Mb/s', cls: 'pos' },
    { lbl: 'UPS', val: '99%', cls: 'pos' },
    { lbl: 'BACKUP', val: '04:13', cls: '' },
  ];
  // Duplicate for seamless loop
  const all = [...items, ...items];
  return (
    <div className="tickerbar">
      <div className="ticker-track">
        {all.map((t, i) => (
          <span key={i} className="ticker-item">
            <span className="lbl">{t.lbl}</span>
            <span className={`val ${t.cls}`}>{t.val}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

// ─── Status bar (bottom) ──────────────────────────────────
function StatusBar({ user = 'me', conn = 'live' }) {
  const [time, setTime] = useState(new Date());
  useEffect(() => {
    const id = setInterval(() => setTime(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  const t = time.toLocaleTimeString('en-US', { hour12: false });
  return (
    <div className="statusbar">
      <span className="text-pos">● {conn.toUpperCase()}</span>
      <span>SYNCED {t}</span>
      <span>USER {user.toUpperCase()}</span>
      <span>WS-1 / HOMELAB.LOCAL</span>
      <span>BUILD 26.05.05+a3f</span>
      <span style={{ marginLeft: 'auto' }}>v1.4.2 · LOCAL-DB</span>
      <span>RAM 412/1024MB</span>
      <span>API 18ms</span>
    </div>
  );
}

// ─── Command palette ──────────────────────────────────────
function CommandPalette({ open, onClose, nav }) {
  const [q, setQ] = useState('');
  const [sel, setSel] = useState(0);
  const inputRef = useRef(null);

  useEffect(() => { if (open) { setQ(''); setSel(0); setTimeout(() => inputRef.current?.focus(), 10); } }, [open]);

  const commands = useMemo(() => {
    const cmds = [
      { section: 'NAV' },
      ...nav.map(n => ({ id: 'nav.' + n.id, label: 'Go to ' + n.label, ico: 'chevR', action: () => nav.find(x=>x.id===n.id)?.go(), kbd: '↵' })),
      { section: 'ACTIONS' },
      { id: 'a.scan', label: 'Scan barcode / QR', ico: 'scan', action: () => window.__inv?.openScanner?.(), kbd: 'S' },
      { id: 'a.newitem', label: 'New item', ico: 'plus', action: () => window.__inv?.toast?.('Form opened (mock)'), kbd: 'I' },
      { id: 'a.newpo', label: 'New purchase order', ico: 'cart', action: () => window.__inv?.toast?.('PO drafted (mock)'), kbd: 'O' },
      { id: 'a.newso', label: 'New pick / ship order', ico: 'truck', action: () => window.__inv?.toast?.('SO created (mock)'), kbd: 'P' },
      { id: 'a.transfer', label: 'New transfer', ico: 'swap', action: () => window.__inv?.toast?.('Transfer started (mock)'), kbd: 'T' },
      { id: 'a.count', label: 'Start cycle count', ico: 'list', action: () => window.__inv?.toast?.('Count opened (mock)'), kbd: 'C' },
      { id: 'a.adjust', label: 'Adjust stock', ico: 'edit', action: () => window.__inv?.toast?.('Adjustment opened (mock)') },
      { id: 'a.label', label: 'Print label', ico: 'print', action: () => window.__inv?.toast?.('Sent to label printer') },
      { id: 'a.export', label: 'Export catalog (CSV)', ico: 'download', action: () => window.__inv?.toast?.('Export started (mock)') },
      { section: 'ITEMS' },
      ...ITEMS.slice(0, 30).map(i => ({ id: 'i.' + i.id, label: i.name, ico: 'box',
        meta: i.sku, action: () => window.__inv?.openItem?.(i.id) })),
    ];
    if (!q) return cmds;
    const lq = q.toLowerCase();
    return cmds.filter(c => c.section || (c.label && c.label.toLowerCase().includes(lq)) || (c.meta && c.meta.toLowerCase().includes(lq)));
  }, [q, nav]);

  const flat = commands.filter(c => !c.section);

  useEffect(() => {
    if (!open) return;
    const onKey = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); onClose(); }
      else if (e.key === 'ArrowDown') { e.preventDefault(); setSel(s => Math.min(flat.length - 1, s + 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setSel(s => Math.max(0, s - 1)); }
      else if (e.key === 'Enter') { e.preventDefault(); flat[sel]?.action?.(); onClose(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, flat, sel, onClose]);

  if (!open) return null;
  let cursor = -1;
  return (
    <div className="palette-mask" onMouseDown={onClose}>
      <div className="palette" onMouseDown={e => e.stopPropagation()}>
        <input ref={inputRef} className="palette-input" placeholder="Type a command, item, SKU, location…"
               value={q} onChange={e => { setQ(e.target.value); setSel(0); }} />
        <div className="palette-list">
          {commands.map((c, i) => {
            if (c.section) return <div key={i} className="palette-section">{c.section}</div>;
            cursor++;
            const Ico = I[c.ico] || I.dot;
            const isSel = cursor === sel;
            return (
              <div key={c.id} className={`palette-item ${isSel ? 'sel' : ''}`}
                   onMouseEnter={() => setSel(cursor)}
                   onClick={() => { c.action?.(); onClose(); }}>
                <span className="ico"><Ico /></span>
                <span>{c.label}</span>
                {c.meta && <span className="meta">{c.meta}</span>}
                {c.kbd && <span className="kbd">{c.kbd}</span>}
              </div>
            );
          })}
          {flat.length === 0 && <div className="empty">No matches</div>}
        </div>
      </div>
    </div>
  );
}

// ─── Scanner modal ────────────────────────────────────────
function ScannerModal({ open, onClose, onScan }) {
  const [phase, setPhase] = useState('scanning'); // scanning, detected, error
  const [code, setCode] = useState('');
  const [item, setItem] = useState(null);
  const [readout, setReadout] = useState('Position barcode in frame…');

  useEffect(() => {
    if (!open) { setPhase('scanning'); setCode(''); setItem(null); setReadout('Position barcode in frame…'); return; }
    let alive = true;
    const messages = [
      'Searching…',
      'Edge detect · 78%',
      'Locked frame…',
      'Decoding EAN-13…',
    ];
    let i = 0;
    const id = setInterval(() => {
      if (!alive) return;
      setReadout(messages[i % messages.length]);
      i++;
      if (i >= messages.length) {
        clearInterval(id);
        // Pick a random scannable item
        setTimeout(() => {
          if (!alive) return;
          const it = ITEMS[Math.floor(Math.random() * 8) + 1];
          setItem(it);
          setCode(it.barcode);
          setPhase('detected');
          setReadout('✓ Match: ' + it.sku);
        }, 600);
      }
    }, 700);
    return () => { alive = false; clearInterval(id); };
  }, [open]);

  if (!open) return null;
  return (
    <div className="modal-mask" onMouseDown={onClose}>
      <div className="modal" style={{ width: 480 }} onMouseDown={e => e.stopPropagation()}>
        <div className="panel-hd">
          <span className="ttl">Scan barcode / QR</span>
          <span className="meta">{phase === 'scanning' ? 'CAMERA · LIVE' : 'MATCH FOUND'}</span>
          <button className="btn icon ghost" onClick={onClose}><I.x /></button>
        </div>
        <div style={{ padding: 18 }}>
          <div className="scanner-frame">
            <div className="scanner-corners"><i/><i/><i/><i/></div>
            {phase === 'scanning' && <div className="scanline" />}
            {phase === 'detected' && (
              <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', color: '#00ff88', fontFamily: 'var(--font-mono)' }}>
                <div style={{ textAlign: 'center' }}>
                  <div style={{ fontSize: 28, marginBottom: 6 }}>✓</div>
                  <div style={{ fontSize: 11 }}>{code}</div>
                </div>
              </div>
            )}
          </div>
          <div className="scanner-readout">{readout}</div>

          {item && (
            <div style={{ marginTop: 14, padding: 12, background: 'var(--bg-2)', border: '1px solid var(--line)', borderRadius: 3 }}>
              <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
                <div className="thumb lg" style={{ background: item.img }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 13 }}>{item.name}</div>
                  <div className="text-mono text-sm text-mute" style={{ marginTop: 2 }}>{item.sku} · {item.brand}</div>
                  <div style={{ display: 'flex', gap: 12, marginTop: 6, fontSize: 11 }}>
                    <span><span className="text-mute">QTY</span> <b className="text-mono">{item.qty}</b></span>
                    <span><span className="text-mute">LOC</span> <b className="text-mono">{LOCATIONS.find(l=>l.id===item.loc[0]?.l)?.code || '—'}</b></span>
                    <span><span className="text-mute">${item.cost}</span></span>
                  </div>
                </div>
              </div>
            </div>
          )}
          <div style={{ display: 'flex', gap: 6, marginTop: 14, justifyContent: 'flex-end' }}>
            {phase === 'scanning' && <button className="btn ghost" onClick={onClose}>Cancel</button>}
            {phase === 'detected' && (
              <>
                <button className="btn" onClick={() => { setPhase('scanning'); setItem(null); setCode(''); }}>Scan another</button>
                <button className="btn" onClick={() => { window.__inv?.toast?.('+1 unit received'); }}>Receive +1</button>
                <button className="btn" onClick={() => { window.__inv?.toast?.('-1 unit picked'); }}>Pick −1</button>
                <button className="btn primary" onClick={() => { onScan?.(item); onClose(); }}>Open item</button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Toasts ───────────────────────────────────────────────
function Toasts({ toasts }) {
  return (
    <div className="toasts">
      {toasts.map(t => (
        <div key={t.id} className={`toast ${t.variant || ''}`}>
          <span className="text-mono text-sm" style={{ color: 'var(--accent)', letterSpacing: '0.06em' }}>
            {t.tag || 'INFO'}
          </span>
          <span>{t.msg}</span>
        </div>
      ))}
    </div>
  );
}

// ─── Heatmap (30 day) ─────────────────────────────────────
function ActivityHeat({ data = SOH_30D }) {
  const max = Math.max(...data);
  const min = Math.min(...data);
  const range = max - min || 1;
  return (
    <div className="heatmap" style={{ gridTemplateColumns: 'repeat(30, 11px)' }}>
      {data.map((v, i) => {
        const t = (v - min) / range;
        const op = 0.18 + t * 0.82;
        return <div key={i} className="heatmap-cell" title={`Day ${i + 1}: ${v}`}
          style={{ background: `color-mix(in oklab, var(--accent) ${Math.round(op * 100)}%, var(--bg-2))` }} />;
      })}
    </div>
  );
}

// ─── QR placeholder (CSS pixel grid) ──────────────────────
function QRCode({ value = '', size = 96 }) {
  // Deterministic 21x21 grid based on value hash
  const grid = useMemo(() => {
    const N = 21;
    const cells = [];
    let h = 0;
    for (let i = 0; i < value.length; i++) h = (h * 31 + value.charCodeAt(i)) >>> 0;
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < N; x++) {
        h = (h * 1103515245 + 12345) >>> 0;
        cells.push((h >>> 16) & 1);
      }
    }
    // Force finder patterns at corners
    const finder = (cx, cy) => {
      for (let dy = -3; dy <= 3; dy++)
        for (let dx = -3; dx <= 3; dx++) {
          const x = cx + dx, y = cy + dy;
          if (x < 0 || y < 0 || x >= N || y >= N) continue;
          const ax = Math.abs(dx), ay = Math.abs(dy);
          const m = Math.max(ax, ay);
          cells[y * N + x] = (m === 1 || m === 3) ? 0 : 1;
          if (m > 3) cells[y * N + x] = 0;
        }
    };
    finder(3, 3); finder(N - 4, 3); finder(3, N - 4);
    return cells;
  }, [value]);
  const cs = size / 21;
  return (
    <svg width={size} height={size} viewBox="0 0 21 21" shapeRendering="crispEdges">
      <rect width="21" height="21" fill="var(--bg-1)" />
      {grid.map((c, i) => c ? <rect key={i} x={i % 21} y={Math.floor(i / 21)} width="1" height="1" fill="var(--fg)" /> : null)}
    </svg>
  );
}

// ─── Barcode (CSS bars) ───────────────────────────────────
function Barcode({ value = '0000000000000', height = 40 }) {
  const bars = useMemo(() => {
    const out = [];
    let h = 0;
    for (let i = 0; i < value.length; i++) h = (h * 31 + value.charCodeAt(i)) >>> 0;
    for (let i = 0; i < 64; i++) {
      h = (h * 1103515245 + 12345) >>> 0;
      const w = ((h >>> 16) % 3) + 1;  // 1-3 px wide
      const b = ((h >>> 8) & 1);
      out.push({ w, b });
    }
    return out;
  }, [value]);
  return (
    <div style={{ display: 'flex', alignItems: 'flex-end', gap: 0, height, fontFamily: 'var(--font-mono)' }}>
      {bars.map((b, i) => (
        <div key={i} style={{
          width: b.w, height: '100%',
          background: b.b ? 'var(--fg)' : 'transparent',
        }} />
      ))}
    </div>
  );
}

Object.assign(window, {
  Sparkline, Pill, StatusPill, BarCell, Ticker, StatusBar,
  CommandPalette, ScannerModal, Toasts, ActivityHeat, QRCode, Barcode,
});
