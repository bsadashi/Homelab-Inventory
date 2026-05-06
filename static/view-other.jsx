// Remaining views: locations, POs, SOs, transfers, suppliers, counts, reports, audit log, scanner kiosk
const { useState: uo_useState, useMemo: uo_useMemo } = React;

// ─── LOCATIONS ──────────────────────────────────────────
function LocationsView({ openItem }) {
  const [activeId, setActiveId] = uo_useState('L1');
  const active = LOCATIONS.find(l => l.id === activeId);

  const occupancy = uo_useMemo(() => {
    const map = {};
    ITEMS.forEach(i => i.loc.forEach(l => {
      if (l.l !== activeId) return;
      map[l.b] = map[l.b] || { items: [], qty: 0 };
      map[l.b].items.push({ ...i, q: l.q });
      map[l.b].qty += l.q;
    }));
    return map;
  }, [activeId]);

  const items = ITEMS.filter(i => i.loc.some(l => l.l === activeId));
  const totalQty = items.reduce((s, i) => s + i.loc.find(l => l.l === activeId)?.q, 0) || 0;
  const totalValue = items.reduce((s, i) => s + (i.loc.find(l => l.l === activeId)?.q || 0) * i.cost, 0);

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '240px 1fr', height: '100%', minHeight: 0 }}>
      {/* Tree sidebar */}
      <div style={{ borderRight: '1px solid var(--line)', background: 'var(--bg-1)', overflow: 'auto' }}>
        <div className="filter-bar" style={{ position: 'sticky', top: 0, zIndex: 1 }}>
          <span className="text-mono text-sm text-mute">LOCATIONS</span>
          <div className="spacer" />
          <button className="btn sm ghost"><I.plus size={11} /></button>
        </div>
        <div className="tree">
          {LOCATIONS.filter(l => !l.parent).map(l => {
            const children = LOCATIONS.filter(c => c.parent === l.id);
            return (
              <div key={l.id}>
                <div className={`tree-node ${activeId === l.id ? 'active' : ''}`} onClick={() => setActiveId(l.id)}>
                  <I.warehouse size={12} />
                  <span>{l.name}</span>
                  <span className="tcode">{l.code}</span>
                </div>
                {children.length > 0 && (
                  <div className="tree-children">
                    {children.map(c => (
                      <div key={c.id} className={`tree-node ${activeId === c.id ? 'active' : ''}`} onClick={() => setActiveId(c.id)}>
                        <I.box size={11} />
                        <span>{c.name}</span>
                        <span className="tcode">{c.code}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Active location */}
      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0, overflow: 'auto' }}>
        <div className="filter-bar">
          <I.location size={14} />
          <span style={{ fontWeight: 600 }}>{active.name}</span>
          <span className="text-mono text-sm text-mute">{active.code}</span>
          <Pill>{active.type}</Pill>
          <div className="spacer" />
          <span className="text-mono text-sm">
            <span className="text-mute">SKUS </span><b>{items.length}</b>
            <span className="text-mute" style={{ marginLeft: 14 }}>UNITS </span><b>{totalQty}</b>
            <span className="text-mute" style={{ marginLeft: 14 }}>VALUE </span><b>${totalValue.toLocaleString()}</b>
          </span>
          <div className="divider" />
          <button className="btn sm"><I.list /> Cycle count</button>
          <button className="btn sm"><I.swap /> Transfer</button>
          <button className="btn sm primary"><I.plus /> Receive here</button>
        </div>

        <div style={{ padding: 14, display: 'grid', gap: 16 }}>
          {/* Bin grid */}
          <div className="panel">
            <div className="panel-hd">
              <span className="ttl">{active.type === 'rack' ? 'RACK UNITS' : 'BINS · SLOTS'}</span>
              <span className="meta">{active.bins.length} positions · {Object.keys(occupancy).length} occupied</span>
            </div>
            <div className="panel-bd">
              {active.type === 'rack' ? (
                <div className="rack">
                  {[...active.bins].reverse().map(u => {
                    const occ = occupancy[u];
                    return (
                      <div key={u} className={`rack-u ${occ ? 'filled' : ''}`}>
                        <span className="un">{u}</span>
                        <span style={{ flex: 1 }}>{occ ? occ.items[0].name : <span style={{ opacity: 0.3 }}>· empty</span>}</span>
                        {occ && <span style={{ color: 'var(--accent)', fontSize: 9 }}>{occ.items[0].brand}</span>}
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="bin-grid">
                  {active.bins.map(b => {
                    const occ = occupancy[b];
                    const cls = !occ ? '' : occ.qty > 10 ? 'full' : 'filled';
                    return (
                      <div key={b} className={`bin ${cls}`} title={occ ? occ.items.map(i=>i.name).join(', ') : 'empty'}>
                        <div className="bcode">{b}</div>
                        {occ && <div className="bcount">{occ.qty}</div>}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

          {/* Items at this location */}
          <div className="panel">
            <div className="panel-hd">
              <span className="ttl">CONTENTS</span>
              <span className="meta">{items.length} SKUs</span>
            </div>
            <div className="panel-bd flush">
              <table className="tbl">
                <thead><tr><th>SKU</th><th>Item</th><th>Bin</th><th className="num">Qty</th><th>Status</th></tr></thead>
                <tbody>
                  {items.map(i => {
                    const here = i.loc.find(l => l.l === activeId);
                    return (
                      <tr key={i.id} onClick={() => openItem(i.id)}>
                        <td className="mono">{i.sku}</td>
                        <td>{i.name}</td>
                        <td className="mono">{here?.b}</td>
                        <td className="num">{here?.q}</td>
                        <td>{i.qty === 0 ? <Pill variant="neg" dot>OOS</Pill> : i.qty < i.min ? <Pill variant="warn" dot>LOW</Pill> : <Pill variant="pos" dot>OK</Pill>}</td>
                      </tr>
                    );
                  })}
                  {items.length === 0 && <tr><td colSpan="5" className="empty">No items at this location</td></tr>}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── PURCHASE ORDERS ─────────────────────────────────────
function PurchaseOrdersView() {
  const [statusFilter, setStatusFilter] = uo_useState('all');
  const [selected, setSelected] = uo_useState(PURCHASE_ORDERS[0].id);
  const filtered = PURCHASE_ORDERS.filter(p => statusFilter === 'all' || p.status === statusFilter);
  const po = PURCHASE_ORDERS.find(p => p.id === selected);
  const sup = SUPPLIERS.find(s => s.id === po?.supplier);

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 480px', height: '100%', minHeight: 0 }}>
      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <div className="filter-bar">
          <span className="text-mono text-sm text-mute">PURCHASE ORDERS</span>
          <div className="divider" />
          <div className="seg">
            {['all','draft','ordered','in-transit','received','cancelled'].map(s => (
              <button key={s} className={statusFilter === s ? 'on' : ''} onClick={() => setStatusFilter(s)}>{s}</button>
            ))}
          </div>
          <div className="spacer" />
          <button className="btn sm"><I.upload /> Import</button>
          <button className="btn sm"><I.download /> Export</button>
          <button className="btn sm primary"><I.plus /> New PO</button>
        </div>
        <div style={{ flex: 1, overflow: 'auto' }}>
          <table className="tbl">
            <thead><tr><th>PO #</th><th>Vendor</th><th>Status</th><th>Created</th><th>ETA</th><th className="num">Lines</th><th className="num">Total</th></tr></thead>
            <tbody>
              {filtered.map(p => {
                const v = SUPPLIERS.find(s => s.id === p.supplier);
                return (
                  <tr key={p.id} className={selected === p.id ? 'selected' : ''} onClick={() => setSelected(p.id)}>
                    <td className="mono text-accent">{p.id}</td>
                    <td>{v?.name}</td>
                    <td><StatusPill status={p.status} /></td>
                    <td className="mono text-sm text-mute">{p.created}</td>
                    <td className="mono text-sm">{p.expected}</td>
                    <td className="num">{p.lines.length}</td>
                    <td className="num">${p.total.toLocaleString()}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
      {/* Detail panel */}
      {po && (
        <div style={{ borderLeft: '1px solid var(--line)', background: 'var(--bg-1)', overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
          <div className="filter-bar">
            <span className="text-mono" style={{ color: 'var(--accent)' }}>{po.id}</span>
            <StatusPill status={po.status} />
            <div className="spacer" />
            <button className="btn sm ghost"><I.print /></button>
            <button className="btn sm ghost"><I.copy /></button>
            <button className="btn sm ghost"><I.edit /></button>
          </div>
          <div style={{ padding: 14, display: 'grid', gap: 14, flex: 1 }}>
            <div>
              <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>VENDOR</div>
              <div style={{ fontWeight: 600 }}>{sup?.name}</div>
              <div className="text-sm text-mute">{sup?.contact} · lead {sup?.leadTime}d · ★ {sup?.rating}</div>
            </div>
            <div>
              <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>SCHEDULE</div>
              <dl className="dl">
                <dt>Created</dt><dd className="text-mono">{po.created}</dd>
                <dt>Expected</dt><dd className="text-mono">{po.expected}</dd>
                {po.received && <><dt>Received</dt><dd className="text-mono text-pos">{po.received}</dd></>}
              </dl>
            </div>
            <div>
              <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>LINE ITEMS</div>
              <table className="tbl">
                <thead><tr><th>SKU</th><th>Item</th><th className="num">Qty</th><th className="num">Cost</th><th className="num">Subtotal</th></tr></thead>
                <tbody>
                  {po.lines.map((ln, i) => {
                    const it = ITEMS.find(x => x.sku === ln.sku);
                    return (
                      <tr key={i}>
                        <td className="mono">{ln.sku}</td>
                        <td>{it?.name || ln.sku}</td>
                        <td className="num">{ln.qty}</td>
                        <td className="num">${ln.cost}</td>
                        <td className="num">${(ln.qty * ln.cost).toLocaleString()}</td>
                      </tr>
                    );
                  })}
                </tbody>
                <tfoot>
                  <tr><td colSpan="4" className="text-mute" style={{ textAlign: 'right', padding: '6px 10px' }}>Subtotal</td><td className="num">${po.total.toLocaleString()}</td></tr>
                  <tr><td colSpan="4" className="text-mute" style={{ textAlign: 'right', padding: '4px 10px' }}>Tax · 0%</td><td className="num">$0</td></tr>
                  <tr><td colSpan="4" style={{ textAlign: 'right', padding: '6px 10px', fontWeight: 600 }}>Total</td><td className="num" style={{ fontWeight: 600, color: 'var(--accent)' }}>${po.total.toLocaleString()}</td></tr>
                </tfoot>
              </table>
            </div>
          </div>
          <div style={{ padding: 10, borderTop: '1px solid var(--line)', display: 'flex', gap: 6 }}>
            {po.status === 'draft' && <><button className="btn sm">Save draft</button><button className="btn sm primary">Send to vendor</button></>}
            {po.status === 'ordered' && <><button className="btn sm danger">Cancel</button><button className="btn sm primary">Mark in-transit</button></>}
            {po.status === 'in-transit' && <button className="btn sm primary">Receive shipment</button>}
            {po.status === 'received' && <><button className="btn sm">Reorder</button><button className="btn sm">Print</button></>}
          </div>
        </div>
      )}
    </div>
  );
}

// ─── SALES / PICK ORDERS ────────────────────────────────
function SalesOrdersView() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <div className="filter-bar">
        <span className="text-mono text-sm text-mute">PICK / SHIP ORDERS</span>
        <div className="divider" />
        <div className="seg">
          {['all','open','picking','shipped'].map(s => (<button key={s} className={s==='all'?'on':''}>{s}</button>))}
        </div>
        <div className="spacer" />
        <button className="btn sm"><I.print /> Pick list</button>
        <button className="btn sm primary"><I.plus /> New pick</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        <table className="tbl">
          <thead><tr><th>SO #</th><th>Project</th><th>Status</th><th>Priority</th><th>Created</th><th className="num">Lines</th><th className="num">Units</th><th>Pick progress</th></tr></thead>
          <tbody>
            {SALES_ORDERS.map(s => {
              const units = s.lines.reduce((a,l) => a + l.qty, 0);
              const progress = s.status === 'shipped' ? 1 : s.status === 'picking' ? 0.5 : 0;
              return (
                <tr key={s.id}>
                  <td className="mono text-accent">{s.id}</td>
                  <td>{s.proj}</td>
                  <td><StatusPill status={s.status} /></td>
                  <td>{s.priority === 'high' ? <Pill variant="neg" dot>HIGH</Pill> : s.priority === 'medium' ? <Pill variant="warn" dot>MED</Pill> : <Pill dot>LOW</Pill>}</td>
                  <td className="mono text-sm text-mute">{s.created}</td>
                  <td className="num">{s.lines.length}</td>
                  <td className="num">{units}</td>
                  <td>
                    <div className="bar-cell">
                      <div className={`bar ${progress === 1 ? 'pos' : ''}`} style={{ minWidth: 100 }}><span style={{ width: `${progress*100}%` }} /></div>
                      <span className="text-mono text-sm text-mute">{Math.round(progress*100)}%</span>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── TRANSFERS ──────────────────────────────────────────
function TransfersView() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div className="filter-bar">
        <span className="text-mono text-sm text-mute">STOCK TRANSFERS</span>
        <div className="spacer" />
        <button className="btn sm primary"><I.swap /> New transfer</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        <table className="tbl">
          <thead><tr><th>TR #</th><th>From</th><th></th><th>To</th><th>Date</th><th>Status</th><th>Items</th></tr></thead>
          <tbody>
            {TRANSFERS.map(t => {
              const from = LOCATIONS.find(l => l.id === t.from);
              const to = LOCATIONS.find(l => l.id === t.to);
              return (
                <tr key={t.id}>
                  <td className="mono text-accent">{t.id}</td>
                  <td>{from?.name} <span className="text-mute text-mono text-sm">{from?.code}</span></td>
                  <td className="text-dim"><I.chevR size={12} /></td>
                  <td>{to?.name} <span className="text-mute text-mono text-sm">{to?.code}</span></td>
                  <td className="mono text-sm">{t.date}</td>
                  <td><StatusPill status={t.status} /></td>
                  <td className="text-mute">{t.lines.map(l => `${l.qty}× ${l.sku}`).join(', ')}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── SUPPLIERS ──────────────────────────────────────────
function SuppliersView() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <div className="filter-bar">
        <span className="text-mono text-sm text-mute">SUPPLIERS</span>
        <div className="spacer" />
        <button className="btn sm primary"><I.plus /> Add vendor</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 14, display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12, alignContent: 'start' }}>
        {SUPPLIERS.map(s => {
          const items = ITEMS.filter(i => i.supplier === s.id);
          const value = items.reduce((a,i) => a + i.qty * i.cost, 0);
          return (
            <div key={s.id} className="panel">
              <div className="panel-hd">
                <span className="ttl text-mono">{s.code}</span>
                <span style={{ fontWeight: 600 }}>{s.name}</span>
                <span className="meta" style={{ marginLeft: 'auto' }}>★ {s.rating}</span>
              </div>
              <div style={{ padding: 12, display: 'grid', gap: 8 }}>
                <div className="text-sm text-mute">{s.contact}</div>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
                  <div><div className="text-mono text-sm text-mute">SKUS</div><div className="text-num" style={{ fontWeight: 600, fontSize: 16 }}>{items.length}</div></div>
                  <div><div className="text-mono text-sm text-mute">LEAD</div><div className="text-num" style={{ fontWeight: 600, fontSize: 16 }}>{s.leadTime}<span className="text-mute" style={{ fontSize: 11 }}> d</span></div></div>
                  <div><div className="text-mono text-sm text-mute">OPEN PO</div><div className="text-num" style={{ fontWeight: 600, fontSize: 16, color: s.openPOs > 0 ? 'var(--warn)' : 'var(--fg)' }}>{s.openPOs}</div></div>
                  <div><div className="text-mono text-sm text-mute">SPEND</div><div className="text-num" style={{ fontWeight: 600, fontSize: 16 }}>${(s.totalSpend/1000).toFixed(1)}<span className="text-mute" style={{ fontSize: 11 }}>k</span></div></div>
                </div>
                <div className="bar-cell">
                  <span className="text-mono text-sm text-mute" style={{ width: 60 }}>VOLUME</span>
                  <div className="bar pos"><span style={{ width: (value / 10000 * 100) + '%' }} /></div>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─── COUNTS / AUDITS ────────────────────────────────────
function CountsView() {
  const [active, setActive] = uo_useState(COUNTS[0].id);
  const c = COUNTS.find(x => x.id === active);
  const loc = LOCATIONS.find(l => l.id === c?.loc);
  const items = ITEMS.filter(i => i.loc.some(l => l.l === c?.loc));

  return (
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', height: '100%', minHeight: 0 }}>
      <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <div className="filter-bar">
          <span className="text-mono text-sm text-mute">CYCLE COUNTS / AUDITS</span>
          <div className="spacer" />
          <button className="btn sm"><I.calendar /> Schedule</button>
          <button className="btn sm primary"><I.list /> Start count</button>
        </div>
        <div style={{ flex: 1, overflow: 'auto' }}>
          <table className="tbl">
            <thead><tr><th>ID</th><th>Location</th><th>Date</th><th>Status</th><th className="num">Counted</th><th className="num">Variance</th></tr></thead>
            <tbody>
              {COUNTS.map(x => {
                const lo = LOCATIONS.find(l => l.id === x.loc);
                return (
                  <tr key={x.id} className={active === x.id ? 'selected' : ''} onClick={() => setActive(x.id)}>
                    <td className="mono text-accent">{x.id}</td>
                    <td>{lo?.name}</td>
                    <td className="mono text-sm">{x.date}</td>
                    <td><StatusPill status={x.status} /></td>
                    <td className="num">{x.counted}</td>
                    <td className={`num ${x.variance > 0 ? 'text-warn' : x.variance < 0 ? 'text-neg' : 'text-mute'}`}>{x.variance > 0 ? '+' : ''}{x.variance}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <div style={{ borderLeft: '1px solid var(--line)', background: 'var(--bg-1)', overflow: 'auto', display: 'flex', flexDirection: 'column' }}>
        <div className="filter-bar">
          <span className="text-mono text-accent">{c?.id}</span>
          <span style={{ fontWeight: 600 }}>{loc?.name}</span>
          <StatusPill status={c?.status} />
          <div className="spacer" />
          <button className="btn sm"><I.scan /> Scan to count</button>
        </div>
        <div style={{ padding: 12 }}>
          <table className="tbl">
            <thead><tr><th>SKU</th><th>Item</th><th>Bin</th><th className="num">Expected</th><th className="num">Counted</th><th className="num">Δ</th></tr></thead>
            <tbody>
              {items.map(i => {
                const here = i.loc.find(l => l.l === c?.loc);
                const counted = c?.status === 'done' ? here?.q + (Math.random() < 0.3 ? (Math.random() < 0.5 ? 1 : -1) : 0) : here?.q;
                const delta = counted - here?.q;
                return (
                  <tr key={i.id} className={delta !== 0 ? 'row-warn' : ''}>
                    <td className="mono">{i.sku}</td>
                    <td>{i.name}</td>
                    <td className="mono">{here?.b}</td>
                    <td className="num">{here?.q}</td>
                    <td className="num">{c?.status === 'scheduled' ? <span className="text-dim">—</span> : counted}</td>
                    <td className={`num ${delta > 0 ? 'text-warn' : delta < 0 ? 'text-neg' : 'text-dim'}`}>{c?.status === 'scheduled' ? '—' : delta === 0 ? '·' : (delta > 0 ? '+' : '') + delta}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

// ─── REPORTS ───────────────────────────────────────────
function ReportsView() {
  const valuation = uo_useMemo(() => {
    const out = {};
    ITEMS.forEach(i => {
      out[i.cat] ||= { units: 0, value: 0, count: 0 };
      out[i.cat].units += i.qty;
      out[i.cat].value += i.qty * i.cost;
      out[i.cat].count += 1;
    });
    return Object.entries(out).sort((a,b) => b[1].value - a[1].value);
  }, []);
  const totalVal = valuation.reduce((s,[,v]) => s + v.value, 0);
  const dead = ITEMS.filter(i => i.qty === 0).length;
  const overstock = ITEMS.filter(i => i.qty > i.max).length;

  return (
    <div style={{ overflow: 'auto', height: '100%', padding: 14, display: 'grid', gridTemplateColumns: 'repeat(12, 1fr)', gap: 14, alignContent: 'start' }}>
      <div className="panel" style={{ gridColumn: 'span 8' }}>
        <div className="panel-hd"><span className="ttl">VALUATION BY CATEGORY</span><span className="meta">total ${totalVal.toLocaleString()}</span></div>
        <div className="panel-bd flush">
          <table className="tbl">
            <thead><tr><th>Category</th><th className="num">SKUs</th><th className="num">Units</th><th className="num">Value</th><th className="num">Avg cost</th><th>Share</th></tr></thead>
            <tbody>
              {valuation.map(([k,v]) => (
                <tr key={k}>
                  <td>{k}</td>
                  <td className="num">{v.count}</td>
                  <td className="num">{v.units}</td>
                  <td className="num">${v.value.toLocaleString()}</td>
                  <td className="num">${(v.value / Math.max(v.units,1)).toFixed(2)}</td>
                  <td><BarCell value={v.value} max={Math.max(...valuation.map(([,x])=>x.value))} min={0} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="panel" style={{ gridColumn: 'span 4' }}>
        <div className="panel-hd"><span className="ttl">HEALTH</span></div>
        <div className="panel-bd" style={{ display: 'grid', gap: 14 }}>
          {[
            ['Dead stock (0 qty)', dead, ITEMS.length, dead === 0 ? 'pos' : 'warn'],
            ['Overstock (>max)', overstock, ITEMS.length, 'info'],
            ['Below min', STATUS.lowStock + STATUS.outOfStock, ITEMS.length, 'warn'],
            ['Serialized coverage', STATUS.serializedUnits, STATUS.totalUnits, 'pos'],
          ].map(([l,n,d,c]) => (
            <div key={l}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 11 }}>
                <span className="text-mute">{l}</span>
                <span className="text-mono"><b>{n}</b><span className="text-mute"> / {d}</span></span>
              </div>
              <div className={`bar ${c}`} style={{ marginTop: 4 }}><span style={{ width: (n/d*100) + '%' }} /></div>
            </div>
          ))}
        </div>
      </div>

      <div className="panel" style={{ gridColumn: 'span 12' }}>
        <div className="panel-hd"><span className="ttl">TOP MOVERS · 30D</span><span className="meta">by units transacted</span></div>
        <div className="panel-bd flush">
          <table className="tbl">
            <thead><tr><th>SKU</th><th>Item</th><th className="num">Received</th><th className="num">Picked</th><th className="num">Net</th><th>Velocity</th></tr></thead>
            <tbody>
              {ITEMS.slice(0, 12).map((i, idx) => {
                const recv = (idx * 7 % 9) + 1;
                const picked = (idx * 13 % 11);
                const net = recv - picked;
                return (
                  <tr key={i.id}>
                    <td className="mono">{i.sku}</td>
                    <td>{i.name}</td>
                    <td className="num text-pos">+{recv}</td>
                    <td className="num text-neg">−{picked}</td>
                    <td className={`num ${net > 0 ? 'text-pos' : net < 0 ? 'text-neg' : 'text-mute'}`}>{net > 0 ? '+' : ''}{net}</td>
                    <td><Sparkline data={[3,5,4,6,5,7,6,8,7,9,8,10].map(v => v + (idx % 3))} width={120} height={24} /></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

// ─── ACTIVITY LOG ──────────────────────────────────────
function AuditLogView() {
  const [type, setType] = uo_useState('all');
  const types = ['all', ...new Set(ACTIVITY.map(a => a.type))];
  const f = ACTIVITY.filter(a => type === 'all' || a.type === type);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div className="filter-bar">
        <span className="text-mono text-sm text-mute">AUDIT LOG · TAMPER-EVIDENT</span>
        <div className="divider" />
        <div className="seg">
          {types.map(t => <button key={t} className={type === t ? 'on' : ''} onClick={() => setType(t)}>{t}</button>)}
        </div>
        <div className="spacer" />
        <button className="btn sm"><I.download /> Export CSV</button>
        <button className="btn sm"><I.shield /> Verify chain</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        <table className="tbl">
          <thead><tr><th>Timestamp</th><th>User</th><th>Type</th><th>Reference</th><th>Description</th><th>Hash</th></tr></thead>
          <tbody>
            {f.map((a, i) => (
              <tr key={i}>
                <td className="mono text-sm">{a.ts}</td>
                <td className="text-mono">{a.user}</td>
                <td><Pill variant="accent">{a.type}</Pill></td>
                <td className="mono text-accent">{a.ref}</td>
                <td>{a.desc}</td>
                <td className="mono text-sm text-dim">{('0' + (i*0xa3f7c5).toString(16)).slice(-7)}…</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── SCANNER · KIOSK ───────────────────────────────────
function ScannerKioskView({ openScanner, openItem }) {
  const recent = ITEMS.slice(0, 6);
  return (
    <div style={{ padding: 24, overflow: 'auto', height: '100%' }}>
      <div style={{ maxWidth: 900, margin: '0 auto' }}>
        <div className="panel" style={{ marginBottom: 14 }}>
          <div className="panel-hd"><span className="ttl">SCANNER · QUICK ACTIONS</span><span className="meta">use scan gun or camera</span></div>
          <div className="panel-bd" style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
            {[
              { l: 'Receive', d: 'Scan to receive +1 unit', i: 'plus', c: 'pos' },
              { l: 'Pick',    d: 'Scan to pick −1 unit',     i: 'minus', c: 'warn' },
              { l: 'Move',    d: 'Scan item, scan target bin',i: 'swap',  c: 'info' },
              { l: 'Lookup',  d: 'Scan to view item details', i: 'eye',  c: '' },
            ].map(a => {
              const Ico = I[a.i];
              return (
                <button key={a.l} className="panel" style={{ background: 'var(--bg-2)', padding: 18, cursor: 'default', textAlign: 'left', alignItems: 'flex-start' }} onClick={openScanner}>
                  <div style={{ width: 28, height: 28, borderRadius: 3, background: 'var(--bg-3)', display: 'grid', placeItems: 'center', color: `var(--${a.c || 'accent'})`, marginBottom: 10 }}>
                    <Ico size={16} />
                  </div>
                  <div style={{ fontSize: 14, fontWeight: 600 }}>{a.l}</div>
                  <div className="text-sm text-mute" style={{ marginTop: 2 }}>{a.d}</div>
                </button>
              );
            })}
          </div>
        </div>

        <div className="panel">
          <div className="panel-hd"><span className="ttl">RECENT SCANS</span><span className="meta">last 5 min</span></div>
          <div className="panel-bd flush">
            <table className="tbl">
              <thead><tr><th>Time</th><th>Code</th><th>Item</th><th>Action</th><th>Result</th></tr></thead>
              <tbody>
                {recent.map((i, idx) => (
                  <tr key={i.id} onClick={() => openItem(i.id)}>
                    <td className="mono text-sm">14:{32 - idx*2}:0{idx}</td>
                    <td className="mono">{i.barcode}</td>
                    <td>{i.name}</td>
                    <td><Pill variant={idx % 2 ? 'pos' : 'info'}>{idx % 2 ? 'Receive +1' : 'Lookup'}</Pill></td>
                    <td className="text-pos">✓ ok</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  LocationsView, PurchaseOrdersView, SalesOrdersView, TransfersView,
  SuppliersView, CountsView, ReportsView, AuditLogView, ScannerKioskView,
});
