// Items catalog: filter bar, dense data table with sortable columns,
// multi-select, bulk actions, and a tabbed detail drawer.
const { useState: ui_useState, useMemo: ui_useMemo, useEffect: ui_useEffect } = React;

function ItemsView({ openItem, openScanner }) {
  const [q, setQ] = ui_useState('');
  const [cats, setCats] = ui_useState(new Set());
  const [stockFilter, setStockFilter] = ui_useState('all'); // all, low, oos, ok
  const [supplierFilter, setSupplierFilter] = ui_useState('');
  const [sortBy, setSortBy] = ui_useState('sku');
  const [sortDir, setSortDir] = ui_useState('asc');
  const [selected, setSelected] = ui_useState(new Set());
  const [colVisible] = ui_useState({
    img: true, sku: true, name: true, cat: true, brand: true, supplier: true,
    qty: true, allocated: true, available: true, min: true, value: true, loc: true, updated: true, tags: true,
  });

  const filtered = ui_useMemo(() => {
    const lq = q.toLowerCase();
    let r = ITEMS.filter(i => {
      if (lq && !i.name.toLowerCase().includes(lq) && !i.sku.toLowerCase().includes(lq)
          && !i.brand.toLowerCase().includes(lq) && !i.barcode.includes(lq)) return false;
      if (cats.size && !cats.has(i.cat)) return false;
      if (stockFilter === 'low' && !(i.qty > 0 && i.qty < i.min)) return false;
      if (stockFilter === 'oos' && i.qty !== 0) return false;
      if (stockFilter === 'ok' && i.qty < i.min) return false;
      if (supplierFilter && i.supplier !== supplierFilter) return false;
      return true;
    });
    r.sort((a, b) => {
      let av, bv;
      switch (sortBy) {
        case 'qty': av = a.qty; bv = b.qty; break;
        case 'value': av = a.qty * a.cost; bv = b.qty * b.cost; break;
        case 'available': av = a.qty - a.allocated; bv = b.qty - b.allocated; break;
        case 'updated': av = a.updated; bv = b.updated; break;
        case 'name': av = a.name; bv = b.name; break;
        case 'cat': av = a.cat; bv = b.cat; break;
        default: av = a[sortBy] || ''; bv = b[sortBy] || '';
      }
      const cmp = av < bv ? -1 : av > bv ? 1 : 0;
      return sortDir === 'asc' ? cmp : -cmp;
    });
    return r;
  }, [q, cats, stockFilter, supplierFilter, sortBy, sortDir]);

  const toggleCat = (c) => { const n = new Set(cats); n.has(c) ? n.delete(c) : n.add(c); setCats(n); };
  const sortIcon = (k) => sortBy !== k ? '↕' : sortDir === 'asc' ? '↑' : '↓';
  const setSort = (k) => { if (sortBy === k) setSortDir(d => d === 'asc' ? 'desc' : 'asc'); else { setSortBy(k); setSortDir('asc'); } };

  const toggleSel = (id) => { const n = new Set(selected); n.has(id) ? n.delete(id) : n.add(id); setSelected(n); };
  const toggleAll = () => setSelected(s => s.size === filtered.length ? new Set() : new Set(filtered.map(i => i.id)));

  const bulkValue = [...selected].reduce((s, id) => {
    const it = ITEMS.find(i => i.id === id); return s + (it ? it.qty * it.cost : 0);
  }, 0);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      {/* Filter bar */}
      <div className="filter-bar">
        <div className="cmd-bar" style={{ maxWidth: 280, flex: 'none' }}>
          <I.search size={12} />
          <input placeholder="Filter SKU, name, brand, barcode…" value={q} onChange={e => setQ(e.target.value)} />
        </div>
        <div className="divider" />
        <div className="seg">
          {['all','low','oos','ok'].map(k => (
            <button key={k} className={stockFilter === k ? 'on' : ''} onClick={() => setStockFilter(k)}>
              {k === 'all' ? 'All' : k === 'low' ? 'Low' : k === 'oos' ? 'OOS' : 'In stock'}
            </button>
          ))}
        </div>
        <div className="divider" />
        <div className="chip-group">
          {CATEGORIES.map(c => (
            <button key={c} className={`chip ${cats.has(c) ? 'on' : ''}`} onClick={() => toggleCat(c)}>{c}</button>
          ))}
        </div>
        <div className="spacer" />
        <span className="text-mono text-sm text-mute">{filtered.length} / {ITEMS.length} items</span>
        <div className="divider" />
        <button className="btn sm ghost" title="Refresh"><I.refresh /></button>
        <button className="btn sm ghost" title="Columns"><I.list /></button>
        <button className="btn sm ghost" title="Export"><I.download /></button>
        <button className="btn sm" onClick={openScanner}><I.scan /> Scan</button>
        <button className="btn sm primary"><I.plus /> New item</button>
      </div>

      {/* Bulk action bar */}
      {selected.size > 0 && (
        <div className="filter-bar" style={{ background: 'color-mix(in oklab, var(--accent) 8%, var(--bg-1))' }}>
          <span className="text-mono text-sm">
            <b className="text-accent">{selected.size} selected</b>
            <span className="text-mute"> · ${bulkValue.toLocaleString()} value</span>
          </span>
          <div className="divider" />
          <button className="btn sm"><I.print /> Print labels</button>
          <button className="btn sm"><I.swap /> Transfer</button>
          <button className="btn sm"><I.cart /> Reorder</button>
          <button className="btn sm"><I.tag /> Tag</button>
          <button className="btn sm"><I.download /> Export</button>
          <button className="btn sm danger"><I.trash /> Archive</button>
          <div className="spacer" />
          <button className="btn sm ghost" onClick={() => setSelected(new Set())}><I.x /> Clear</button>
        </div>
      )}

      {/* Table */}
      <div style={{ flex: 1, overflow: 'auto', background: 'var(--bg)' }}>
        <table className="tbl">
          <thead>
            <tr>
              <th style={{ width: 24 }}><span className={`check ${selected.size === filtered.length && filtered.length > 0 ? 'on' : ''}`} onClick={toggleAll} /></th>
              <th style={{ width: 32 }}></th>
              <th onClick={() => setSort('sku')} className={sortBy === 'sku' ? 'sorted' : ''}>SKU <span className="sort">{sortIcon('sku')}</span></th>
              <th onClick={() => setSort('name')} className={sortBy === 'name' ? 'sorted' : ''}>Item <span className="sort">{sortIcon('name')}</span></th>
              <th onClick={() => setSort('cat')} className={sortBy === 'cat' ? 'sorted' : ''}>Category <span className="sort">{sortIcon('cat')}</span></th>
              <th>Brand</th>
              <th>Vendor</th>
              <th className="num" onClick={() => setSort('qty')} style={{ cursor: 'pointer' }}>Qty <span className="sort">{sortIcon('qty')}</span></th>
              <th className="num" onClick={() => setSort('available')} style={{ cursor: 'pointer' }}>Avail <span className="sort">{sortIcon('available')}</span></th>
              <th className="num">Min</th>
              <th>Stock</th>
              <th className="num" onClick={() => setSort('value')} style={{ cursor: 'pointer' }}>Value <span className="sort">{sortIcon('value')}</span></th>
              <th>Location</th>
              <th onClick={() => setSort('updated')} className={sortBy === 'updated' ? 'sorted' : ''}>Updated <span className="sort">{sortIcon('updated')}</span></th>
              <th>Tags</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map(i => {
              const sup = SUPPLIERS.find(s => s.id === i.supplier);
              const avail = i.qty - i.allocated;
              const stockClass = i.qty === 0 ? 'row-neg' : i.qty < i.min ? 'row-warn' : '';
              const isSel = selected.has(i.id);
              return (
                <tr key={i.id} className={`${stockClass} ${isSel ? 'selected' : ''}`}>
                  <td onClick={(e) => { e.stopPropagation(); toggleSel(i.id); }}>
                    <span className={`check ${isSel ? 'on' : ''}`} />
                  </td>
                  <td onClick={() => openItem(i.id)}>
                    <span className="thumb" style={{ background: i.img }} />
                  </td>
                  <td className="mono" onClick={() => openItem(i.id)}>{i.sku}</td>
                  <td onClick={() => openItem(i.id)}>
                    <span style={{ fontWeight: 500 }}>{i.name}</span>
                    {i.variants && <span className="text-mute text-sm"> · {i.variants.length} variants</span>}
                  </td>
                  <td className="text-mute" onClick={() => openItem(i.id)}>{i.cat}</td>
                  <td className="text-mute" onClick={() => openItem(i.id)}>{i.brand}</td>
                  <td className="mono text-sm" onClick={() => openItem(i.id)}>{sup?.code}</td>
                  <td className="num" onClick={() => openItem(i.id)}>{i.qty}</td>
                  <td className="num" onClick={() => openItem(i.id)}>
                    <span className={i.allocated > 0 ? 'text-warn' : ''}>{avail}</span>
                  </td>
                  <td className="num text-mute" onClick={() => openItem(i.id)}>{i.min}</td>
                  <td onClick={() => openItem(i.id)}>
                    {i.qty === 0
                      ? <Pill variant="neg" dot>OOS</Pill>
                      : i.qty < i.min
                      ? <Pill variant="warn" dot>LOW</Pill>
                      : i.qty >= i.max
                      ? <Pill variant="info" dot>FULL</Pill>
                      : <Pill variant="pos" dot>OK</Pill>}
                  </td>
                  <td className="num" onClick={() => openItem(i.id)}>${(i.qty * i.cost).toLocaleString()}</td>
                  <td className="mono text-sm text-mute" onClick={() => openItem(i.id)}>
                    {i.loc.length === 0 ? '—' : i.loc.length === 1
                      ? `${LOCATIONS.find(l=>l.id===i.loc[0].l)?.code}/${i.loc[0].b}`
                      : `${i.loc.length} locations`}
                  </td>
                  <td className="mono text-sm text-mute" onClick={() => openItem(i.id)}>{i.updated}</td>
                  <td onClick={() => openItem(i.id)}>
                    {i.tags.slice(0, 2).map(t => <Pill key={t}>{t}</Pill>)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {filtered.length === 0 && <div className="empty">No items match</div>}
      </div>

      {/* Footer summary */}
      <div className="filter-bar" style={{ borderTop: '1px solid var(--line)', borderBottom: 0 }}>
        <span className="text-mono text-sm text-mute">
          Σ qty <b className="text-num" style={{ color: 'var(--fg)' }}> {filtered.reduce((s,i)=>s+i.qty,0)}</b>
          <span style={{ marginLeft: 14 }}>Σ value <b className="text-num" style={{ color: 'var(--fg)' }}>${filtered.reduce((s,i)=>s+i.qty*i.cost,0).toLocaleString()}</b></span>
          <span style={{ marginLeft: 14 }}>Σ allocated <b className="text-num" style={{ color: 'var(--fg)' }}>{filtered.reduce((s,i)=>s+i.allocated,0)}</b></span>
        </span>
        <div className="spacer" />
        <span className="text-mono text-sm text-dim">Updated 14:32:08 · auto-refresh 60s</span>
      </div>
    </div>
  );
}

// ─── Item drawer ─────────────────────────────────────────
function ItemDrawer({ id, onClose }) {
  const item = ITEMS.find(i => i.id === id);
  const [tab, setTab] = ui_useState('overview');
  ui_useEffect(() => {
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  if (!item) return null;
  const sup = SUPPLIERS.find(s => s.id === item.supplier);
  const avail = item.qty - item.allocated;

  return (
    <>
      <div className="drawer-mask" onClick={onClose} />
      <div className="drawer">
        {/* Header */}
        <div className="drawer-hd">
          <div className="thumb lg" style={{ background: item.img, flexShrink: 0 }} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="text-mono text-sm text-mute" style={{ letterSpacing: '0.04em' }}>{item.sku}</div>
            <div style={{ fontSize: 16, fontWeight: 600, marginTop: 2 }}>{item.name}</div>
            <div className="text-sm text-mute" style={{ marginTop: 2 }}>
              {item.brand} · {item.cat}
              {item.tags.map(t => <Pill key={t}>{t}</Pill>)}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 4 }}>
            <button className="btn icon ghost" title="Print label"><I.print /></button>
            <button className="btn icon ghost" title="Edit"><I.edit /></button>
            <button className="btn icon ghost" title="More"><I.dotsV /></button>
            <button className="btn icon ghost" onClick={onClose}><I.x /></button>
          </div>
        </div>

        {/* Quick stats strip */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(5, 1fr)', borderBottom: '1px solid var(--line)' }}>
          {[
            ['ON HAND', item.qty, ''],
            ['ALLOC', item.allocated, item.allocated > 0 ? 'text-warn' : ''],
            ['AVAIL', avail, avail < item.min ? 'text-warn' : 'text-pos'],
            ['MIN/MAX', `${item.min}/${item.max}`, 'text-mute'],
            ['VALUE', '$' + (item.qty * item.cost).toLocaleString(), ''],
          ].map(([l, v, cls], idx) => (
            <div key={l} style={{ padding: '10px 14px', borderRight: idx < 4 ? '1px solid var(--line)' : 0 }}>
              <div className="text-mono text-sm text-mute" style={{ fontSize: 9.5, letterSpacing: '0.06em' }}>{l}</div>
              <div className={`text-num ${cls}`} style={{ fontSize: 16, fontWeight: 600, marginTop: 2 }}>{v}</div>
            </div>
          ))}
        </div>

        {/* Tabs */}
        <div className="drawer-tabs">
          {['overview','stock','history','codes','specs'].map(t => (
            <div key={t} className={`drawer-tab ${tab === t ? 'active' : ''}`} onClick={() => setTab(t)}>{t}</div>
          ))}
        </div>

        {/* Body */}
        <div className="drawer-bd">
          {tab === 'overview' && <ItemOverview item={item} sup={sup} />}
          {tab === 'stock' && <ItemStock item={item} />}
          {tab === 'history' && <ItemHistory item={item} />}
          {tab === 'codes' && <ItemCodes item={item} />}
          {tab === 'specs' && <ItemSpecs item={item} />}
        </div>

        {/* Footer actions */}
        <div style={{ padding: 10, borderTop: '1px solid var(--line)', display: 'flex', gap: 6, background: 'var(--bg-1)' }}>
          <button className="btn sm"><I.plus /> Receive</button>
          <button className="btn sm"><I.minus /> Pick</button>
          <button className="btn sm"><I.swap /> Transfer</button>
          <button className="btn sm"><I.list /> Count</button>
          <div className="spacer" />
          <button className="btn sm primary"><I.cart /> Reorder ×{Math.max(item.max - item.qty, 1)}</button>
        </div>
      </div>
    </>
  );
}

function ItemOverview({ item, sup }) {
  return (
    <div style={{ padding: 16, display: 'grid', gap: 16 }}>
      <div>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>IDENTIFIERS</div>
        <dl className="dl">
          <dt>SKU</dt><dd className="text-mono">{item.sku}</dd>
          <dt>Barcode</dt><dd className="text-mono">{item.barcode}</dd>
          <dt>Internal ID</dt><dd className="text-mono text-mute">{item.id}</dd>
          <dt>Updated</dt><dd className="text-mono text-mute">{item.updated}</dd>
        </dl>
      </div>

      <div>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>SUPPLIER</div>
        <dl className="dl">
          <dt>Vendor</dt><dd>{sup?.name} <span className="text-mute">· {sup?.code}</span></dd>
          <dt>Cost / unit</dt><dd className="text-mono">${item.cost}</dd>
          <dt>Lead time</dt><dd className="text-mono">{sup?.leadTime} days</dd>
          <dt>Last received</dt><dd className="text-mono text-mute">{item.updated}</dd>
        </dl>
      </div>

      <div>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>STOCK FORECAST · 30D</div>
        <Sparkline data={SOH_30D.map(v => v + (item.id.charCodeAt(2) % 5))} width={460} height={50} fill />
      </div>

      {item.variants && (
        <div>
          <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>VARIANTS · {item.variants.length}</div>
          <table className="tbl">
            <thead><tr><th>Variant</th><th className="num">Qty</th><th>Bar</th></tr></thead>
            <tbody>
              {item.variants.map(v => (
                <tr key={v.name}>
                  <td>{v.name}</td>
                  <td className="num">{v.q}</td>
                  <td><BarCell value={v.q} max={Math.max(...item.variants.map(x=>x.q))} min={1} /></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function ItemStock({ item }) {
  return (
    <div style={{ padding: 16, display: 'grid', gap: 16 }}>
      <div>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>STOCK BY LOCATION</div>
        <table className="tbl">
          <thead><tr><th>Location</th><th>Bin</th><th className="num">Qty</th><th>Serials</th></tr></thead>
          <tbody>
            {item.loc.length === 0 && <tr><td colSpan="4" className="empty" style={{ padding: 20 }}>Not stocked anywhere</td></tr>}
            {item.loc.map((l, i) => {
              const loc = LOCATIONS.find(x => x.id === l.l);
              return (
                <tr key={i}>
                  <td>{loc?.name}</td>
                  <td className="mono">{l.b}</td>
                  <td className="num">{l.q}</td>
                  <td className="mono text-sm text-mute">{l.serial?.join(', ') || '—'}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {item.lots && (
        <div>
          <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>LOTS / EXPIRY</div>
          <table className="tbl">
            <thead><tr><th>Lot #</th><th>Expiry</th><th className="num">Qty</th></tr></thead>
            <tbody>
              {item.lots.map(l => (
                <tr key={l.lot}>
                  <td className="mono">{l.lot}</td>
                  <td className="mono text-sm">{l.exp || <span className="text-dim">—</span>}</td>
                  <td className="num">{l.q}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>REORDER POLICY</div>
        <dl className="dl">
          <dt>Min</dt><dd className="text-mono">{item.min}</dd>
          <dt>Max</dt><dd className="text-mono">{item.max}</dd>
          <dt>Auto-reorder</dt><dd>Enabled · trigger when qty &lt; {item.min}</dd>
          <dt>Reorder qty</dt><dd className="text-mono">{item.max - item.min}</dd>
        </dl>
      </div>
    </div>
  );
}

function ItemHistory({ item }) {
  // Synthesize per-item activity
  const events = [
    { ts: item.updated + ' 14:32', type: 'receive', desc: '+1 unit · PO-1041', delta: '+1' },
    { ts: '2026-04-22 09:15', type: 'pick',     desc: '−1 unit · SO-217 · Edge cluster', delta: '−1' },
    { ts: '2026-04-15 11:40', type: 'transfer', desc: 'L4 → L1 · 1 unit',  delta: '·' },
    { ts: '2026-04-08 16:02', type: 'count',    desc: 'Cycle count · no variance',  delta: '·' },
    { ts: '2026-03-30 10:11', type: 'create',   desc: 'Item created in catalog',  delta: '·' },
  ];
  return (
    <div style={{ padding: 0 }}>
      {events.map((e, i) => (
        <div key={i} className="act-row" style={{ gridTemplateColumns: '110px 60px 1fr 30px' }}>
          <span className="ts">{e.ts}</span>
          <span className="type">{e.type}</span>
          <span>{e.desc}</span>
          <span className="text-mono text-sm" style={{ color: e.delta.startsWith('+') ? 'var(--pos)' : e.delta.startsWith('−') ? 'var(--neg)' : 'var(--fg-mute)' }}>{e.delta}</span>
        </div>
      ))}
    </div>
  );
}

function ItemCodes({ item }) {
  return (
    <div style={{ padding: 16, display: 'flex', gap: 24, alignItems: 'flex-start' }}>
      <div style={{ flex: 1 }}>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 8, letterSpacing: '0.06em' }}>QR · {item.id}</div>
        <div style={{ background: 'var(--bg)', padding: 14, border: '1px solid var(--line)', display: 'inline-block' }}>
          <QRCode value={item.id + ':' + item.barcode} size={140} />
        </div>
        <div className="text-mono text-sm" style={{ marginTop: 8 }}>homelab://item/{item.id}</div>
      </div>
      <div style={{ flex: 1 }}>
        <div className="text-mono text-sm text-mute" style={{ marginBottom: 8, letterSpacing: '0.06em' }}>BARCODE · EAN-13</div>
        <div style={{ background: 'var(--bg)', padding: '14px 16px', border: '1px solid var(--line)', display: 'inline-block' }}>
          <Barcode value={item.barcode} height={60} />
          <div className="text-mono text-sm" style={{ marginTop: 8, letterSpacing: '0.18em', textAlign: 'center' }}>{item.barcode}</div>
        </div>
        <div style={{ marginTop: 14, display: 'flex', gap: 6 }}>
          <button className="btn sm"><I.print /> Print 1</button>
          <button className="btn sm"><I.print /> Print sheet</button>
          <button className="btn sm"><I.download /> PNG</button>
          <button className="btn sm"><I.copy /> Copy</button>
        </div>
        <div className="text-mono text-sm text-mute" style={{ marginTop: 14 }}>
          Label format: Brother PT TZe-231 · 12mm · 4 lines
        </div>
      </div>
    </div>
  );
}

function ItemSpecs({ item }) {
  // Mock specs based on category
  const specs = {
    Networking: [['Type','Switch · L3'],['Ports','24× RJ45 + 2× SFP+'],['PoE','PoE+ 400W']],
    Compute:    [['CPU','varies'],['RAM','varies'],['Form factor','varies']],
    Storage:    [['Capacity','—'],['Interface','—'],['RPM/IOPS','—']],
    Cables:     [['Length','—'],['Standard','—'],['Shielding','—']],
    Power:      [['Wattage','—'],['Connector','—'],['Outlets','—']],
  }[item.cat] || [['Notes','—']];
  return (
    <div style={{ padding: 16 }}>
      <div className="text-mono text-sm text-mute" style={{ marginBottom: 6, letterSpacing: '0.06em' }}>SPECIFICATIONS</div>
      <dl className="dl">
        {specs.map(([k, v]) => (<><dt key={k+'k'}>{k}</dt><dd key={k+'v'}>{v}</dd></>))}
      </dl>
      <div className="text-mono text-sm text-mute" style={{ margin: '20px 0 6px', letterSpacing: '0.06em' }}>NOTES</div>
      <p className="text-mute" style={{ lineHeight: 1.5 }}>
        Operational notes appear here. Documentation, links, install scripts, runbook references, datasheet PDFs.
      </p>
    </div>
  );
}

Object.assign(window, { ItemsView, ItemDrawer });
