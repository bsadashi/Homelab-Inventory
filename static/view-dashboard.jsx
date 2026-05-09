// Dashboard — KPIs, alerts, activity, low-stock, sparklines, heatmap
const { useState: ud_useState, useMemo: ud_useMemo } = React;

function Dashboard({ layout, openItem }) {
  // Layout variants: 'standard', 'ops', 'analytics'
  const cards = LAYOUT_DEFS[layout] || LAYOUT_DEFS.standard;
  return (
    <div className="dash">
      {cards.map((c, i) => (
        <div key={i} className="panel" style={{ gridColumn: `span ${c.span}`, gridRow: c.row ? `span ${c.row}` : '' }}>
          {c.render({ openItem })}
        </div>
      ))}
    </div>
  );
}

// ─── KPI cards ───────────────────────────────────────────
function KPI({ label, value, unit, delta, deltaLabel, sparkData, sparkAccent, deltaVariant }) {
  return (
    <div className="kpi">
      <div className="lbl">{label}</div>
      <div className="val">{value}{unit && <span className="unit">{unit}</span>}</div>
      {delta != null && (
        <div className={`delta ${deltaVariant || (delta > 0 ? 'pos' : delta < 0 ? 'neg' : '')}`}>
          {delta > 0 ? '▲' : delta < 0 ? '▼' : '·'} {Math.abs(delta)}{deltaLabel ? ` ${deltaLabel}` : ''}
        </div>
      )}
      {sparkData && <div className="spark"><Sparkline data={sparkData} width={220} height={28} fill accent={sparkAccent} /></div>}
    </div>
  );
}

// ─── Critical alerts panel ──────────────────────────────
function AlertsPanel({ openItem }) {
  const lows = ITEMS.filter(i => i.qty < i.min).sort((a,b) => (a.qty/a.min) - (b.qty/b.min));
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">CRITICAL · LOW STOCK</span>
        <span className="meta">{lows.length} items</span>
        <span className="actions">
          <button className="btn sm ghost"><I.refresh /></button>
          <button className="btn sm ghost"><I.dotsH /></button>
        </span>
      </div>
      <div className="panel-bd flush">
        <table className="tbl">
          <thead>
            <tr>
              <th>SKU</th><th>Item</th><th className="num">QTY</th><th className="num">MIN</th>
              <th>Status</th><th>Reorder</th>
            </tr>
          </thead>
          <tbody>
            {lows.map(i => {
              const sup = SUPPLIERS.find(s => s.id === i.supplier);
              return (
                <tr key={i.id} className={i.qty === 0 ? 'row-neg' : 'row-warn'} onClick={() => openItem?.(i.id)}>
                  <td className="mono">{i.sku}</td>
                  <td>{i.name}</td>
                  <td className="num">{i.qty}</td>
                  <td className="num text-mute">{i.min}</td>
                  <td>{i.qty === 0 ? <Pill variant="neg" dot>OOS</Pill> : <Pill variant="warn" dot>LOW</Pill>}</td>
                  <td className="mono text-mute">{sup?.code || '—'} · {sup?.leadTime}d</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ─── Activity feed panel ────────────────────────────────
function ActivityPanel() {
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">ACTIVITY · LIVE</span>
        <span className="meta">last 48h · {ACTIVITY.length} events</span>
        <span className="actions"><button className="btn sm ghost"><I.dotsH /></button></span>
      </div>
      <div className="panel-bd flush">
        {ACTIVITY.map((a, i) => (
          <div key={i} className="act-row">
            <span className="ts">{a.ts.split(' ')[1]}</span>
            <span className="type">{a.type}</span>
            <span>{a.desc}</span>
            <span className="ref">{a.ref}</span>
          </div>
        ))}
      </div>
    </>
  );
}

// ─── Open POs panel ─────────────────────────────────────
function OpenPOsPanel() {
  const open = PURCHASE_ORDERS.filter(p => p.status !== 'received' && p.status !== 'cancelled');
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">OPEN PURCHASE ORDERS</span>
        <span className="meta">{open.length} active · ${open.reduce((s,o)=>s+o.total,0).toLocaleString()} on order</span>
      </div>
      <div className="panel-bd flush">
        <table className="tbl">
          <thead>
            <tr><th>PO</th><th>Vendor</th><th>Status</th><th>ETA</th><th className="num">Lines</th><th className="num">Total</th></tr>
          </thead>
          <tbody>
            {open.map(po => {
              const sup = SUPPLIERS.find(s => s.id === po.supplier);
              return (
                <tr key={po.id}>
                  <td className="mono text-accent">{po.id}</td>
                  <td>{sup?.name}</td>
                  <td><StatusPill status={po.status} /></td>
                  <td className="mono text-sm">{po.expected}</td>
                  <td className="num">{po.lines.length}</td>
                  <td className="num">${po.total.toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ─── Stock-on-hand by category ──────────────────────────
function CategoryStockPanel() {
  const byCat = ud_useMemo(() => {
    const out = {};
    ITEMS.forEach(i => {
      out[i.cat] ||= { units: 0, value: 0, skus: 0 };
      out[i.cat].units += i.qty;
      out[i.cat].value += i.qty * i.cost;
      out[i.cat].skus += 1;
    });
    return Object.entries(out).sort((a,b) => b[1].value - a[1].value);
  }, []);
  const max = Math.max(...byCat.map(([,v]) => v.value));
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">STOCK BY CATEGORY</span>
        <span className="meta">value</span>
      </div>
      <div className="panel-bd flush">
        <table className="tbl">
          <thead><tr><th>Category</th><th className="num">SKUs</th><th className="num">Units</th><th className="num">Value</th><th>Distribution</th></tr></thead>
          <tbody>
            {byCat.map(([k, v]) => (
              <tr key={k}>
                <td>{k}</td>
                <td className="num text-mute">{v.skus}</td>
                <td className="num">{v.units}</td>
                <td className="num">${v.value.toLocaleString()}</td>
                <td><BarCell value={v.value} max={max} min={0} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ─── Rack visualization ─────────────────────────────────
function RackPanel() {
  // Build a 16U rack view from the Main Rack
  const rack = LOCATIONS[0]; // RACK-A
  const items = ITEMS.filter(i => i.loc.some(l => l.l === rack.id));
  const occupy = {};
  items.forEach(i => i.loc.filter(l => l.l === rack.id).forEach(l => { occupy[l.b] = i; }));
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">RACK-A · MAIN</span>
        <span className="meta">16U · {Object.keys(occupy).length}/16 used</span>
      </div>
      <div className="panel-bd">
        <div className="rack">
          {[...rack.bins].reverse().map(u => {
            const it = occupy[u];
            return (
              <div key={u} className={`rack-u ${it ? 'filled' : ''}`}>
                <span className="un">{u}</span>
                <span style={{ flex: 1 }}>{it ? it.name : <span style={{ opacity: 0.4 }}>· empty</span>}</span>
                {it && <span style={{ color: 'var(--accent)', fontSize: 9 }}>{it.brand}</span>}
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}

// ─── Heatmap panel ─────────────────────────────────────
function StockTrendPanel() {
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">STOCK ON HAND · 30D</span>
        <span className="meta">unit count</span>
      </div>
      <div className="panel-bd">
        <div style={{ height: 80, marginBottom: 10 }}>
          <Sparkline data={SOH_30D} width={500} height={80} fill />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <ActivityHeat data={SOH_30D} />
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 14, fontFamily: 'var(--font-mono)', fontSize: 10 }}>
            <span><span className="text-mute">MIN </span><b className="text-num">{Math.min(...SOH_30D)}</b></span>
            <span><span className="text-mute">AVG </span><b className="text-num">{Math.round(SOH_30D.reduce((a,b)=>a+b,0)/SOH_30D.length)}</b></span>
            <span><span className="text-mute">MAX </span><b className="text-num">{Math.max(...SOH_30D)}</b></span>
            <span className="text-pos">▲ +{SOH_30D[SOH_30D.length-1] - SOH_30D[0]}</span>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── Pending tasks (counts, transfers) ──────────────────
function TasksPanel() {
  const tasks = [
    ...COUNTS.filter(c => c.status !== 'done').map(c => ({ kind: 'count', id: c.id, label: 'Count ' + (LOCATIONS.find(l=>l.id===c.loc)?.code || c.loc), date: c.date, status: c.status })),
    ...TRANSFERS.filter(t => t.status !== 'done').map(t => ({ kind: 'transfer', id: t.id, label: `Transfer ${LOCATIONS.find(l=>l.id===t.from)?.code} → ${LOCATIONS.find(l=>l.id===t.to)?.code}`, date: t.date, status: t.status })),
    ...SALES_ORDERS.filter(s => s.status === 'open' || s.status === 'picking').map(s => ({ kind: 'pick', id: s.id, label: 'Pick · ' + s.proj, date: s.created, status: s.status, priority: s.priority })),
  ];
  return (
    <>
      <div className="panel-hd">
        <span className="ttl">QUEUE · MY TASKS</span>
        <span className="meta">{tasks.length} pending</span>
      </div>
      <div className="panel-bd flush">
        <table className="tbl">
          <thead><tr><th>ID</th><th>Task</th><th>Status</th><th>When</th></tr></thead>
          <tbody>
            {tasks.map(t => (
              <tr key={t.kind+t.id}>
                <td className="mono text-accent">{t.id}</td>
                <td>{t.label}{t.priority === 'high' && <Pill variant="neg" dot> hi </Pill>}</td>
                <td><StatusPill status={t.status} /></td>
                <td className="mono text-sm text-mute">{t.date}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ─── Mini KPIs row ──────────────────────────────────────
function KPIBlock({ which }) {
  const blocks = {
    soh:    { label: 'STOCK ON HAND',  value: STATUS.totalUnits.toLocaleString(), unit: 'units', delta: 12, deltaLabel: 'vs 30d', sparkData: SOH_30D },
    val:    { label: 'INVENTORY VALUE', value: '$' + STATUS.totalValue.toLocaleString(), delta: 4.8, deltaLabel: '%', sparkData: SOH_30D.map(v => v * 18 + 700), deltaVariant: 'pos' },
    sku:    { label: 'ACTIVE SKUS',  value: STATUS.totalSKUs, unit: 'items', delta: 1, deltaLabel: 'new this wk' },
    low:    { label: 'LOW · OUT',    value: `${STATUS.lowStock} · ${STATUS.outOfStock}`, delta: -2, deltaLabel: 'vs last wk', deltaVariant: 'pos', sparkData: [9,7,7,6,6,5,4,5,4,3], sparkAccent: 'var(--warn)' },
    open:   { label: 'OPEN PO · SO',  value: `${STATUS.openPOs} · ${STATUS.openSOs}`, delta: 0, deltaLabel: 'this wk' },
    serial: { label: 'SERIALIZED',   value: STATUS.serializedUnits, unit: 'units', delta: 4, deltaLabel: 'this mo' },
  };
  return <KPI {...blocks[which]} />;
}

// Layout variants. Each card returns the JSX inside its panel.
const LAYOUT_DEFS = {
  standard: [
    { span: 2, render: () => <KPIBlock which="soh" /> },
    { span: 2, render: () => <KPIBlock which="val" /> },
    { span: 2, render: () => <KPIBlock which="sku" /> },
    { span: 2, render: () => <KPIBlock which="low" /> },
    { span: 2, render: () => <KPIBlock which="open" /> },
    { span: 2, render: () => <KPIBlock which="serial" /> },
    { span: 8, render: ({ openItem }) => <AlertsPanel openItem={openItem} /> },
    { span: 4, render: () => <RackPanel /> },
    { span: 6, render: () => <ActivityPanel /> },
    { span: 6, render: () => <CategoryStockPanel /> },
    { span: 8, render: () => <StockTrendPanel /> },
    { span: 4, render: () => <TasksPanel /> },
    { span: 12, render: () => <OpenPOsPanel /> },
  ],
  ops: [
    { span: 3, render: () => <KPIBlock which="low" /> },
    { span: 3, render: () => <KPIBlock which="open" /> },
    { span: 3, render: () => <KPIBlock which="soh" /> },
    { span: 3, render: () => <KPIBlock which="val" /> },
    { span: 6, render: ({ openItem }) => <AlertsPanel openItem={openItem} /> },
    { span: 6, render: () => <TasksPanel /> },
    { span: 12, render: () => <ActivityPanel /> },
    { span: 12, render: () => <OpenPOsPanel /> },
  ],
  analytics: [
    { span: 4, render: () => <KPIBlock which="soh" /> },
    { span: 4, render: () => <KPIBlock which="val" /> },
    { span: 4, render: () => <KPIBlock which="serial" /> },
    { span: 12, render: () => <StockTrendPanel /> },
    { span: 6, render: () => <CategoryStockPanel /> },
    { span: 6, render: () => <RackPanel /> },
  ],
};

window.Dashboard = Dashboard;
