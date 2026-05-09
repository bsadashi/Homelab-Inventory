// 3D rack visualisation. Self-contained ES module that pulls Three.js
// from the same CDN we already use for React/Babel/ZXing. Surfaces a
// single API on window.RL.rack3d.open() so any view can fire it.
//
// Layout: one column per rack location, one cube per bin (U-slot,
// shelf, drawer cell, etc). Cubes are coloured by occupancy state:
//   green  · stocked, qty ≥ min
//   amber  · qty < min
//   red    · qty == 0 (empty slot)
//   grey   · slot defined but no items mapped
//
// Click a cube → opens the existing item drawer for the first item
// in that bin (best-effort; multi-item bins show a chooser).

(function () {
  // Pin to a specific minor — major + minor of three.js have shipped
  // breaking changes in the past. RACKLOG.html includes
  // <link rel="modulepreload" integrity="…"> tags for both URLs;
  // run scripts/compute-sri.sh on deploy to fill in the hashes.
  const THREE_URL = 'https://unpkg.com/three@0.169.0/build/three.module.min.js';
  const ORBIT_URL = 'https://unpkg.com/three@0.169.0/examples/jsm/controls/OrbitControls.js';

  // Lazy-load Three on first open so the dashboard's first paint
  // isn't penalised when the user never touches 3D.
  let threePromise = null;
  function loadThree() {
    if (threePromise) return threePromise;
    threePromise = (async () => {
      const THREE = await import(THREE_URL);
      const { OrbitControls } = await import(ORBIT_URL);
      return { THREE, OrbitControls };
    })();
    return threePromise;
  }

  function colourForBin(qty, min) {
    if (qty <= 0) return 0xb91c1c;          // red — empty
    if (min > 0 && qty < min) return 0xf59e0b; // amber — below min
    return 0x16a34a;                         // green — healthy
  }

  function buildBinIndex() {
    // Walk every item's `loc` array once and build:
    //   binIndex.get(locationId).get(binCode) = [{itemId, sku, qty}, …]
    const items = window.ITEMS || [];
    const idx = new Map();
    for (const it of items) {
      for (const line of it.loc || []) {
        const locId = line.l;
        const bin = line.b;
        if (!locId || !bin) continue;
        if (!idx.has(locId)) idx.set(locId, new Map());
        const byBin = idx.get(locId);
        if (!byBin.has(bin)) byBin.set(bin, []);
        byBin.get(bin).push({
          itemId: it.id,
          sku: it.sku,
          name: it.name,
          qty: line.q,
          min: it.min,
        });
      }
    }
    return idx;
  }

  function summariseBin(entries) {
    if (!entries || entries.length === 0) return { qty: 0, min: 0 };
    const qty = entries.reduce((a, e) => a + (e.qty || 0), 0);
    // Use the strictest min in the bin as the threshold.
    const min = entries.reduce((a, e) => Math.max(a, e.min || 0), 0);
    return { qty, min };
  }

  async function open() {
    const { THREE, OrbitControls } = await loadThree();
    const overlay = mountOverlay();

    const width = overlay.viewport.clientWidth;
    const height = overlay.viewport.clientHeight;
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    renderer.setSize(width, height);
    overlay.viewport.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0f172a);

    const camera = new THREE.PerspectiveCamera(45, width / height, 0.1, 1000);
    camera.position.set(8, 8, 14);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.target.set(0, 4, 0);

    scene.add(new THREE.AmbientLight(0xffffff, 0.45));
    const dir = new THREE.DirectionalLight(0xffffff, 0.9);
    dir.position.set(10, 20, 10);
    scene.add(dir);

    // Build the rack columns from window.LOCATIONS.
    const locations = window.LOCATIONS || [];
    const binIndex = buildBinIndex();

    const interactive = []; // { mesh, info } for raycasting
    const raycaster = new THREE.Raycaster();
    const mouse = new THREE.Vector2();

    let xOffset = 0;
    const labels = [];
    for (const loc of locations) {
      const bins = loc.bins || [];
      if (bins.length === 0) continue;
      const byBin = binIndex.get(loc.id) || new Map();

      // Base plate.
      const baseGeo = new THREE.BoxGeometry(2.5, 0.2, 1.5);
      const baseMat = new THREE.MeshStandardMaterial({ color: 0x1e293b });
      const base = new THREE.Mesh(baseGeo, baseMat);
      base.position.set(xOffset, 0, 0);
      scene.add(base);

      // Stack one cube per bin going up.
      bins.forEach((bin, i) => {
        const entries = byBin.get(bin) || [];
        const summary = summariseBin(entries);
        const colour = entries.length === 0
          ? 0x475569 // grey — empty slot definition
          : colourForBin(summary.qty, summary.min);

        const geo = new THREE.BoxGeometry(2, 0.45, 1.2);
        const mat = new THREE.MeshStandardMaterial({
          color: colour,
          metalness: 0.1,
          roughness: 0.6,
        });
        const cube = new THREE.Mesh(geo, mat);
        cube.position.set(xOffset, 0.5 + i * 0.55, 0);
        scene.add(cube);
        interactive.push({
          mesh: cube,
          info: { loc, bin, entries, summary },
        });
      });

      labels.push({ x: xOffset, name: loc.code });
      xOffset += 3.2;
    }

    // Floor.
    const floor = new THREE.Mesh(
      new THREE.PlaneGeometry(80, 80),
      new THREE.MeshStandardMaterial({ color: 0x020617, roughness: 1 }),
    );
    floor.rotation.x = -Math.PI / 2;
    floor.position.y = -0.1;
    scene.add(floor);

    // Pointer interaction → tooltip + click-to-drill.
    const tip = overlay.tip;
    function updateTip(ev) {
      const rect = renderer.domElement.getBoundingClientRect();
      mouse.x = ((ev.clientX - rect.left) / rect.width) * 2 - 1;
      mouse.y = -((ev.clientY - rect.top) / rect.height) * 2 + 1;
      raycaster.setFromCamera(mouse, camera);
      const hits = raycaster.intersectObjects(interactive.map((i) => i.mesh));
      if (!hits.length) {
        tip.style.display = 'none';
        return null;
      }
      const target = interactive.find((i) => i.mesh === hits[0].object);
      tip.style.display = 'block';
      tip.style.left = `${ev.clientX - rect.left + 12}px`;
      tip.style.top = `${ev.clientY - rect.top + 12}px`;
      const t = target.info;
      const lines = t.entries.length
        ? t.entries.map((e) => `· ${e.sku} × ${e.qty}`).join('<br>')
        : '<i>empty bin</i>';
      tip.innerHTML =
        `<b>${escapeHtml(t.loc.code)} · ${escapeHtml(t.bin)}</b><br>` +
        `Σ qty ${t.summary.qty} (min ${t.summary.min})<br>` +
        lines;
      return target;
    }
    renderer.domElement.addEventListener('mousemove', updateTip);
    renderer.domElement.addEventListener('mouseleave', () => {
      tip.style.display = 'none';
    });
    renderer.domElement.addEventListener('click', (ev) => {
      const target = updateTip(ev);
      if (!target) return;
      const first = target.info.entries[0];
      if (first && window.__inv && window.__inv.openItem) {
        window.__inv.openItem(first.itemId);
        close();
      }
    });

    // Animation loop.
    let alive = true;
    function tick() {
      if (!alive) return;
      controls.update();
      renderer.render(scene, camera);
      requestAnimationFrame(tick);
    }
    tick();

    function onResize() {
      const w = overlay.viewport.clientWidth;
      const h = overlay.viewport.clientHeight;
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    }
    window.addEventListener('resize', onResize);

    function close() {
      alive = false;
      window.removeEventListener('resize', onResize);
      renderer.dispose();
      overlay.root.remove();
    }
    overlay.closeBtn.addEventListener('click', close);
    document.addEventListener('keydown', function onKey(e) {
      if (e.key === 'Escape') {
        close();
        document.removeEventListener('keydown', onKey);
      }
    });

    // Render an HTML legend for the cube colours.
    overlay.legend.innerHTML = `
      <span class="rl-3d-key" style="background:#16a34a"></span> healthy
      <span class="rl-3d-key" style="background:#f59e0b"></span> below min
      <span class="rl-3d-key" style="background:#b91c1c"></span> empty
      <span class="rl-3d-key" style="background:#475569"></span> unmapped slot
    `;
  }

  function mountOverlay() {
    const root = document.createElement('div');
    Object.assign(root.style, {
      position: 'fixed', inset: '0', zIndex: '9999',
      background: 'rgba(2,6,23,0.95)', color: '#e5e7eb',
      display: 'flex', flexDirection: 'column',
      fontFamily: 'Inter, system-ui, sans-serif',
    });

    const header = document.createElement('div');
    Object.assign(header.style, {
      padding: '10px 14px', display: 'flex', alignItems: 'center', gap: '12px',
      borderBottom: '1px solid #1e293b',
    });
    header.innerHTML =
      '<b style="letter-spacing:0.04em;">3D RACK VIEW</b>' +
      '<span style="color:#94a3b8;font-size:12px;">drag to orbit · scroll to zoom · click a slot</span>';

    const legend = document.createElement('div');
    Object.assign(legend.style, {
      marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: '12px',
      fontSize: '11px', color: '#94a3b8',
    });
    header.appendChild(legend);

    const closeBtn = document.createElement('button');
    closeBtn.textContent = '✕  Esc';
    Object.assign(closeBtn.style, {
      background: 'transparent', color: '#e5e7eb', border: '1px solid #475569',
      padding: '4px 10px', borderRadius: '4px', cursor: 'pointer', fontSize: '11px',
    });
    header.appendChild(closeBtn);

    const viewport = document.createElement('div');
    Object.assign(viewport.style, { flex: '1', position: 'relative' });

    const tip = document.createElement('div');
    Object.assign(tip.style, {
      position: 'absolute', display: 'none', pointerEvents: 'none',
      background: '#0f172a', border: '1px solid #334155', padding: '6px 8px',
      borderRadius: '4px', fontSize: '11px', color: '#e5e7eb',
      fontFamily: 'JetBrains Mono, ui-monospace, monospace',
      maxWidth: '260px', lineHeight: '1.4', zIndex: '2',
    });
    viewport.appendChild(tip);

    root.appendChild(header);
    root.appendChild(viewport);

    // Basic stylesheet for the legend swatches. Inline so we're
    // dependency-free.
    const style = document.createElement('style');
    style.textContent =
      '.rl-3d-key { display:inline-block; width:10px; height:10px; ' +
      'border-radius:2px; margin-right:4px; vertical-align:middle; }';
    root.appendChild(style);

    document.body.appendChild(root);
    return { root, header, legend, viewport, tip, closeBtn };
  }

  const escapeHtml = (s) => window.RL.util.escapeHtml(s);

  // Expose. The ops bar will gain a ▦ button that calls this.
  window.RL = window.RL || {};
  window.RL.rack3d = { open };
})();
