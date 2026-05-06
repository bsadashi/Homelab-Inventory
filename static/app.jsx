// App shell — sidebar nav, top bar, view router, modals, tweaks.
const { useState: ua_useState, useEffect: ua_useEffect, useRef: ua_useRef, useCallback: ua_useCallback } = React;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "theme": "dark",
  "density": "regular",
  "accent": "amber",
  "rail": "wide",
  "layout": "standard",
  "ticker": true
}/*EDITMODE-END*/;

const NAV = [
  { id: 'dashboard', label: 'Dashboard',        icon: 'dashboard', section: 'OVERVIEW' },
  { id: 'items',     label: 'Items',            icon: 'box',       section: 'CATALOG', badge: STATUS.totalSKUs },
  { id: 'scanner',   label: 'Scanner',          icon: 'scan',      section: 'CATALOG' },
  { id: 'locations', label: 'Locations',        icon: 'warehouse', section: 'CATALOG' },
  { id: 'pos',       label: 'Purchase orders', icon: 'cart',      section: 'OPERATIONS', badge: STATUS.openPOs, badgeVariant: 'warn' },
  { id: 'sos',       label: 'Pick & ship',     icon: 'truck',     section: 'OPERATIONS', badge: STATUS.openSOs },
  { id: 'transfers', label: 'Transfers',        icon: 'swap',      section: 'OPERATIONS', badge: STATUS.pendingTransfers, badgeVariant: 'warn' },
  { id: 'counts',    label: 'Counts & audits',  icon: 'list',      section: 'OPERATIONS', badge: STATUS.scheduledCounts },
  { id: 'suppliers', label: 'Suppliers',        icon: 'users',     section: 'NETWORK', badge: SUPPLIERS.length },
  { id: 'reports',   label: 'Reports',          icon: 'chart',     section: 'INSIGHTS' },
  { id: 'audit',     label: 'Activity log',     icon: 'history',   section: 'INSIGHTS' },
];

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [view, setView] = ua_useState('dashboard');
  const [openItemId, setOpenItemId] = ua_useState(null);
  const [scannerOpen, setScannerOpen] = ua_useState(false);
  const [paletteOpen, setPaletteOpen] = ua_useState(false);
  const [toasts, setToasts] = ua_useState([]);
  const tIdRef = ua_useRef(0);

  // Apply theme tokens to root
  ua_useEffect(() => {
    document.documentElement.dataset.theme = t.theme;
    document.documentElement.dataset.d = t.density;
    document.documentElement.dataset.accent = t.accent;
    document.documentElement.dataset.rail = t.rail;
  }, [t.theme, t.density, t.accent, t.rail]);

  // Toast helper
  const toast = ua_useCallback((msg, opts = {}) => {
    const id = ++tIdRef.current;
    setToasts(s => [...s, { id, msg, tag: opts.tag || 'OK', variant: opts.variant || 'pos' }]);
    setTimeout(() => setToasts(s => s.filter(x => x.id !== id)), 2400);
  }, []);

  // Expose globals for keyboard shortcuts / palette
  ua_useEffect(() => {
    window.__inv = {
      openScanner: () => setScannerOpen(true),
      openItem:    (id) => setOpenItemId(id),
      goto:        (v) => setView(v),
      toast,
    };
  }, [toast]);

  // Keyboard shortcuts
  ua_useEffect(() => {
    const onKey = (e) => {
      // Don't trigger while typing in inputs
      const inField = ['INPUT','TEXTAREA'].includes(e.target.tagName);
      if ((e.key === 'k' || e.key === 'K') && (e.metaKey || e.ctrlKey)) {
        e.preventDefault(); setPaletteOpen(true);
      } else if (e.key === '/' && !inField) {
        e.preventDefault(); setPaletteOpen(true);
      } else if ((e.key === 's' || e.key === 'S') && !inField && !e.metaKey && !e.ctrlKey) {
        e.preventDefault(); setScannerOpen(true);
      } else if (e.key >= '1' && e.key <= '9' && e.altKey) {
        e.preventDefault();
        const v = NAV[parseInt(e.key) - 1];
        if (v) setView(v.id);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const navItems = NAV.map(n => ({ ...n, go: () => setView(n.id) }));
  const activeNav = NAV.find(n => n.id === view);

  return (
    <>
      <Ticker />
      <div className="app">
        {/* Sidebar */}
        <div className="sidebar">
          <div className="brand">
            <div className="brand-mark">RL</div>
            <div className="brand-name">RACKLOG</div>
            <div className="brand-meta">v1.4</div>
          </div>
          <div className="nav">
            {(() => {
              const items = [];
              let lastSection = null;
              navItems.forEach(n => {
                if (n.section !== lastSection) {
                  items.push(<div key={'s' + n.section} className="nav-section">{n.section}</div>);
                  lastSection = n.section;
                }
                const Ico = I[n.icon];
                items.push(
                  <div key={n.id} className={`nav-item ${view === n.id ? 'active' : ''}`} onClick={() => setView(n.id)}>
                    <span className="nav-icon"><Ico /></span>
                    <span className="nav-label">{n.label}</span>
                    {n.badge != null && <span className={`nav-badge ${n.badgeVariant || ''}`}>{n.badge}</span>}
                  </div>
                );
              });
              return items;
            })()}
          </div>
          <div className="sidebar-foot">
            <span className="user-dot">M</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 500 }}>me@homelab</div>
              <div className="text-mono text-sm text-dim">admin · ws-1</div>
            </div>
            <button className="btn icon ghost" onClick={() => setTweak('theme', t.theme === 'dark' ? 'light' : 'dark')}>
              {t.theme === 'dark' ? <I.sun /> : <I.moon />}
            </button>
          </div>
        </div>

        {/* Workspace */}
        <div className="workspace">
          <div className="topbar">
            <div className="crumbs">
              <span>HOMELAB</span>
              <span className="sep">/</span>
              <b>{activeNav?.label || view}</b>
            </div>
            <div style={{ flex: 1 }} />
            <div className="cmd-bar" onClick={() => setPaletteOpen(true)}>
              <I.search size={12} />
              <input placeholder="Search items, SKUs, locations…  ⌘K" readOnly />
              <span className="kbd">⌘K</span>
            </div>
            <button className="btn sm" onClick={() => setScannerOpen(true)}><I.scan /> Scan <span className="kbd">S</span></button>
            <button className="btn icon ghost" title="Notifications" onClick={() => toast('3 low stock alerts · 1 PO arrives today', { tag: 'NOTIFY', variant: 'warn' })}>
              <I.bell />
            </button>
            <button className="btn icon ghost" title="Settings"><I.settings /></button>
          </div>

          <div className="work-body">
            {view === 'dashboard' && <Dashboard layout={t.layout} openItem={setOpenItemId} />}
            {view === 'items'     && <ItemsView openItem={setOpenItemId} openScanner={() => setScannerOpen(true)} />}
            {view === 'scanner'   && <ScannerKioskView openScanner={() => setScannerOpen(true)} openItem={setOpenItemId} />}
            {view === 'locations' && <LocationsView openItem={setOpenItemId} />}
            {view === 'pos'       && <PurchaseOrdersView />}
            {view === 'sos'       && <SalesOrdersView />}
            {view === 'transfers' && <TransfersView />}
            {view === 'counts'    && <CountsView />}
            {view === 'suppliers' && <SuppliersView />}
            {view === 'reports'   && <ReportsView />}
            {view === 'audit'     && <AuditLogView />}
          </div>
        </div>

        <StatusBar />
      </div>

      {/* Modals & overlays */}
      {openItemId && <ItemDrawer id={openItemId} onClose={() => setOpenItemId(null)} />}
      <ScannerModal open={scannerOpen} onClose={() => setScannerOpen(false)} onScan={(it) => { setOpenItemId(it.id); }} />
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} nav={navItems} />
      <Toasts toasts={toasts} />

      <TweaksPanel>
        <TweakSection label="Appearance">
          <TweakRadio label="Theme" value={t.theme} options={['dark','light']} onChange={v => setTweak('theme', v)} />
          <TweakColor label="Accent" value={t.accent}
                      options={[
                        { value:'amber',   color:'#f59e0b' },
                        { value:'emerald', color:'#10b981' },
                        { value:'azure',   color:'#3b82f6' },
                        { value:'violet',  color:'#a855f7' },
                        { value:'crimson', color:'#ef4444' },
                      ].map(x => x.color)}
                      onChange={(hex) => {
                        const map = { '#f59e0b':'amber','#10b981':'emerald','#3b82f6':'azure','#a855f7':'violet','#ef4444':'crimson' };
                        setTweak('accent', map[hex.toLowerCase()] || 'amber');
                      }} />
        </TweakSection>
        <TweakSection label="Layout">
          <TweakRadio label="Density"  value={t.density} options={['compact','regular','comfortable']} onChange={v => setTweak('density', v)} />
          <TweakRadio label="Sidebar"  value={t.rail}    options={['wide','narrow']}                  onChange={v => setTweak('rail', v)} />
        </TweakSection>
        <TweakSection label="Dashboard">
          <TweakRadio label="Layout"   value={t.layout}  options={['standard','ops','analytics']}     onChange={v => setTweak('layout', v)} />
          <TweakToggle label="Ticker bar" value={t.ticker} onChange={v => setTweak('ticker', v)} />
        </TweakSection>
        <TweakButton label="Trigger demo alert"
                     onClick={() => toast('Auto-PO drafted: 4× Cat6A 3ft below min', { tag: 'AUTO-PO', variant: 'warn' })} />
      </TweaksPanel>

      <style>{`
        ${!t.ticker ? '.tickerbar { display: none; } .app { grid-template-rows: 1fr 22px; }' : ''}
      `}</style>
    </>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
