// Homelab inventory dataset — items, locations, suppliers, POs, etc.
// Real homelab gear: networking, servers, storage, cables, IoT, power, lab tools.

const LOCATIONS = [
  { id: 'L1', code: 'RACK-A',   name: 'Main Rack',         type: 'rack',     parent: null, bins: ['U1','U2','U3','U4','U5','U6','U7','U8','U9','U10','U11','U12','U13','U14','U15','U16'] },
  { id: 'L2', code: 'RACK-B',   name: 'Secondary Rack',    type: 'rack',     parent: null, bins: ['U1','U2','U3','U4','U5','U6','U7','U8','U9','U10','U11','U12'] },
  { id: 'L3', code: 'SHELF-1',  name: 'Wall Shelf · Lab',  type: 'shelf',    parent: null, bins: ['S1-A','S1-B','S1-C','S1-D'] },
  { id: 'L4', code: 'BIN-NET',  name: 'Networking Bin',    type: 'bin',      parent: 'L3', bins: ['N1','N2','N3','N4','N5','N6','N7','N8'] },
  { id: 'L5', code: 'BIN-CBL',  name: 'Cable Drawer',      type: 'drawer',   parent: 'L3', bins: ['C1','C2','C3','C4','C5','C6'] },
  { id: 'L6', code: 'BOX-PARTS',name: 'Spare Parts Box',   type: 'box',      parent: 'L3', bins: ['P1','P2','P3','P4'] },
  { id: 'L7', code: 'DESK-LAB', name: 'Lab Bench',         type: 'desk',     parent: null, bins: ['D1','D2','D3'] },
  { id: 'L8', code: 'CLOSET',   name: 'Storage Closet',    type: 'room',     parent: null, bins: ['SH1','SH2','SH3','SH4','FLOOR'] },
];

const SUPPLIERS = [
  { id: 'V1', code: 'AMZN', name: 'Amazon',          contact: 'support@amazon.com',  leadTime: 2,  rating: 4.6, openPOs: 2, totalSpend: 8420 },
  { id: 'V2', code: 'NEWEGG', name: 'Newegg',        contact: 'orders@newegg.com',   leadTime: 4,  rating: 4.3, openPOs: 1, totalSpend: 3210 },
  { id: 'V3', code: 'MONOP', name: 'Monoprice',      contact: 'cs@monoprice.com',    leadTime: 5,  rating: 4.4, openPOs: 0, totalSpend: 1140 },
  { id: 'V4', code: 'UI',   name: 'Ubiquiti Store',  contact: 'support@ui.com',      leadTime: 7,  rating: 4.7, openPOs: 1, totalSpend: 5680 },
  { id: 'V5', code: 'SVRP', name: 'ServerPartDeals', contact: 'sales@spd.com',       leadTime: 6,  rating: 4.5, openPOs: 0, totalSpend: 2890 },
  { id: 'V6', code: 'ADAF', name: 'Adafruit',        contact: 'support@adafruit.com',leadTime: 3,  rating: 4.8, openPOs: 1, totalSpend: 612 },
  { id: 'V7', code: 'DIGK', name: 'DigiKey',         contact: 'orders@digikey.com',  leadTime: 2,  rating: 4.9, openPOs: 0, totalSpend: 410 },
  { id: 'V8', code: 'EBAY', name: 'eBay (used)',     contact: 'n/a',                 leadTime: 8,  rating: 3.9, openPOs: 0, totalSpend: 1820 },
];

const CATEGORIES = [
  'Networking', 'Compute', 'Storage', 'Cables', 'Power', 'IoT / Sensors',
  'Tools', 'Consumables', 'Spare parts', 'Media',
];

// Items — the canonical catalog. qty is sum across locations (computed but stored for speed).
const ITEMS = [
  // — Networking —
  { id:'I001', sku:'NET-USW-PRO-24', name:'UniFi Switch Pro 24 PoE', cat:'Networking', brand:'Ubiquiti', supplier:'V4', cost:799, price:799, unit:'ea', min:1, max:2, qty:1, allocated:0, barcode:'810010071316', loc:[{l:'L1',b:'U6',q:1,serial:['SN-USW24-A1F0']}], variants:null, lots:null, tags:['core','poe'], updated:'2026-04-30', img:'#1f6feb' },
  { id:'I002', sku:'NET-UDM-PRO',   name:'UniFi Dream Machine Pro', cat:'Networking', brand:'Ubiquiti', supplier:'V4', cost:479, price:479, unit:'ea', min:1, max:1, qty:1, allocated:0, barcode:'810010071125', loc:[{l:'L1',b:'U4',q:1,serial:['SN-UDMP-9F2C']}], variants:null, lots:null, tags:['core'], updated:'2026-04-12', img:'#1f6feb' },
  { id:'I003', sku:'NET-UAP-U6E',   name:'UniFi U6 Enterprise AP', cat:'Networking', brand:'Ubiquiti', supplier:'V4', cost:279, price:279, unit:'ea', min:2, max:4, qty:3, allocated:1, barcode:'810010073242', loc:[{l:'L1',b:'U2',q:1,serial:['SN-U6E-001']},{l:'L4',b:'N1',q:2,serial:['SN-U6E-002','SN-U6E-003']}], variants:null, lots:null, tags:['wifi6e'], updated:'2026-05-01', img:'#1f6feb' },
  { id:'I004', sku:'NET-MS-10G',    name:'10G SFP+ Module DAC',     cat:'Networking', brand:'Mikrotik', supplier:'V2', cost:35, price:35, unit:'ea', min:4, max:12, qty:8, allocated:2, barcode:'724560731120', loc:[{l:'L4',b:'N3',q:8}], variants:[{name:'1m',sku:'-1M',q:5},{name:'3m',sku:'-3M',q:3}], lots:null, tags:['sfp'], updated:'2026-04-28', img:'#0ea5e9' },
  { id:'I005', sku:'NET-GBIC-LR',   name:'10G SFP+ LR Optical',     cat:'Networking', brand:'FS.com',   supplier:'V2', cost:48, price:48, unit:'ea', min:2, max:6, qty:1, allocated:0, barcode:'600346122003', loc:[{l:'L4',b:'N3',q:1}], variants:null, lots:null, tags:['sfp','low-stock'], updated:'2026-05-02', img:'#0ea5e9' },

  // — Compute —
  { id:'I010', sku:'SVR-R730-XD',   name:'Dell R730xd 12-bay',      cat:'Compute', brand:'Dell',     supplier:'V5', cost:849, price:849, unit:'ea', min:1, max:2, qty:1, allocated:0, barcode:'884116236001', loc:[{l:'L1',b:'U10',q:1,serial:['SN-R730-7VKZQ02']}], variants:null, lots:null, tags:['core','virt'], updated:'2026-03-20', img:'#7c3aed' },
  { id:'I011', sku:'SVR-PI5-8GB',   name:'Raspberry Pi 5 (8GB)',    cat:'Compute', brand:'Raspberry Pi', supplier:'V6', cost:80, price:80, unit:'ea', min:2, max:5, qty:4, allocated:1, barcode:'640522071104', loc:[{l:'L4',b:'N5',q:3,serial:['SN-PI5-001','SN-PI5-002','SN-PI5-003']},{l:'L7',b:'D1',q:1,serial:['SN-PI5-004']}], variants:null, lots:null, tags:['edge'], updated:'2026-04-22', img:'#16a34a' },
  { id:'I012', sku:'SVR-MNF-BX',    name:'Mini PC · N100 16GB',     cat:'Compute', brand:'Beelink',  supplier:'V1', cost:189, price:189, unit:'ea', min:1, max:3, qty:2, allocated:0, barcode:'196336714421', loc:[{l:'L3',b:'S1-A',q:2,serial:['SN-BL-N100-A','SN-BL-N100-B']}], variants:null, lots:null, tags:['cluster'], updated:'2026-04-15', img:'#7c3aed' },
  { id:'I013', sku:'SVR-NUC-I5',    name:'Intel NUC i5 13th gen',   cat:'Compute', brand:'Intel',    supplier:'V8', cost:420, price:420, unit:'ea', min:1, max:2, qty:0, allocated:0, barcode:'735858505017', loc:[], variants:null, lots:null, tags:['out-of-stock'], updated:'2026-03-08', img:'#7c3aed' },

  // — Storage —
  { id:'I020', sku:'STR-WD18-RED',  name:'WD Red Pro 18TB HDD',     cat:'Storage', brand:'WD',       supplier:'V2', cost:329, price:329, unit:'ea', min:2, max:8, qty:6, allocated:6, barcode:'718037893570', loc:[{l:'L1',b:'U10',q:6,serial:['SN-WD18-A','SN-WD18-B','SN-WD18-C','SN-WD18-D','SN-WD18-E','SN-WD18-F']}], variants:null, lots:null, tags:['nas'], updated:'2026-04-29', img:'#dc2626' },
  { id:'I021', sku:'STR-MX-2T-NV',  name:'Samsung 990 Pro 2TB',     cat:'Storage', brand:'Samsung',  supplier:'V1', cost:179, price:179, unit:'ea', min:2, max:6, qty:2, allocated:0, barcode:'887276731278', loc:[{l:'L4',b:'N6',q:2,serial:['SN-990-X1','SN-990-X2']}], variants:null, lots:null, tags:['nvme'], updated:'2026-04-18', img:'#dc2626' },
  { id:'I022', sku:'STR-SDXC-256',  name:'SanDisk Extreme 256GB',   cat:'Storage', brand:'SanDisk',  supplier:'V1', cost:38, price:38, unit:'ea', min:4, max:12, qty:7, allocated:1, barcode:'619659170103', loc:[{l:'L6',b:'P1',q:7}], variants:null, lots:null, tags:[], updated:'2026-04-25', img:'#dc2626' },

  // — Cables —
  { id:'I030', sku:'CBL-CAT6A-3',   name:'Cat6A Patch · 3 ft',      cat:'Cables', brand:'Monoprice', supplier:'V3', cost:5, price:5, unit:'ea', min:10, max:30, qty:18, allocated:4, barcode:'889028198412', loc:[{l:'L5',b:'C1',q:18}], variants:[{name:'Black',sku:'-BLK',q:8},{name:'Blue',sku:'-BLU',q:6},{name:'Yellow',sku:'-YEL',q:4}], lots:null, tags:[], updated:'2026-05-03', img:'#475569' },
  { id:'I031', sku:'CBL-CAT6A-7',   name:'Cat6A Patch · 7 ft',      cat:'Cables', brand:'Monoprice', supplier:'V3', cost:8, price:8, unit:'ea', min:10, max:30, qty:22, allocated:6, barcode:'889028198429', loc:[{l:'L5',b:'C1',q:22}], variants:[{name:'Black',sku:'-BLK',q:10},{name:'Blue',sku:'-BLU',q:8},{name:'Red',sku:'-RED',q:4}], lots:null, tags:[], updated:'2026-05-03', img:'#475569' },
  { id:'I032', sku:'CBL-USB-C2C',   name:'USB-C to USB-C 100W',     cat:'Cables', brand:'Anker',     supplier:'V1', cost:14, price:14, unit:'ea', min:6, max:20, qty:11, allocated:0, barcode:'194644026010', loc:[{l:'L5',b:'C2',q:11}], variants:[{name:'1m',sku:'-1M',q:6},{name:'2m',sku:'-2M',q:5}], lots:null, tags:[], updated:'2026-04-28', img:'#475569' },
  { id:'I033', sku:'CBL-HDMI-21',   name:'HDMI 2.1 8K · 6 ft',      cat:'Cables', brand:'Cable Matters', supplier:'V1', cost:19, price:19, unit:'ea', min:4, max:10, qty:3, allocated:0, barcode:'810149732204', loc:[{l:'L5',b:'C3',q:3}], variants:null, lots:null, tags:['low-stock'], updated:'2026-05-01', img:'#475569' },
  { id:'I034', sku:'CBL-DAC-3M',    name:'DAC 10G · 3m',            cat:'Cables', brand:'FS.com',    supplier:'V2', cost:25, price:25, unit:'ea', min:2, max:8, qty:2, allocated:0, barcode:'600346160418', loc:[{l:'L5',b:'C4',q:2}], variants:null, lots:null, tags:['low-stock'], updated:'2026-04-12', img:'#475569' },

  // — Power —
  { id:'I040', sku:'PWR-UPS-1500',  name:'CyberPower UPS 1500VA',   cat:'Power', brand:'CyberPower',supplier:'V1', cost:219, price:219, unit:'ea', min:1, max:2, qty:1, allocated:0, barcode:'649532902002', loc:[{l:'L1',b:'U16',q:1,serial:['SN-UPS-1500-N3K9']}], variants:null, lots:null, tags:['core'], updated:'2026-02-04', img:'#facc15' },
  { id:'I041', sku:'PWR-PDU-RACK',  name:'PDU 12-outlet 15A',       cat:'Power', brand:'Tripp Lite',supplier:'V1', cost:89, price:89, unit:'ea', min:1, max:2, qty:2, allocated:0, barcode:'037332191670', loc:[{l:'L1',b:'U15',q:1,serial:['SN-PDU-001']},{l:'L2',b:'U12',q:1,serial:['SN-PDU-002']}], variants:null, lots:null, tags:[], updated:'2026-02-12', img:'#facc15' },
  { id:'I042', sku:'PWR-AAA-PK4',   name:'AAA Battery (4-pk)',      cat:'Power', brand:'Energizer', supplier:'V1', cost:6, price:6, unit:'pk', min:8, max:24, qty:14, allocated:2, barcode:'039800013606', loc:[{l:'L6',b:'P2',q:14}], variants:null, lots:[{lot:'L26-04',exp:'2030-12-31',q:14}], tags:[], updated:'2026-04-09', img:'#facc15' },

  // — IoT —
  { id:'I050', sku:'IOT-ESP32',     name:'ESP32-S3 DevKit',         cat:'IoT / Sensors', brand:'Espressif', supplier:'V6', cost:12, price:12, unit:'ea', min:5, max:20, qty:9, allocated:2, barcode:'810030410126', loc:[{l:'L6',b:'P3',q:9}], variants:null, lots:null, tags:['firmware'], updated:'2026-04-30', img:'#16a34a' },
  { id:'I051', sku:'IOT-ZB-TEMP',   name:'Zigbee Temp/Humidity',    cat:'IoT / Sensors', brand:'Aqara',  supplier:'V1', cost:18, price:18, unit:'ea', min:4, max:12, qty:6, allocated:0, barcode:'190781000000', loc:[{l:'L6',b:'P3',q:6}], variants:null, lots:null, tags:[], updated:'2026-05-02', img:'#10b981' },
  { id:'I052', sku:'IOT-MOT-PIR',   name:'PIR Motion Sensor',       cat:'IoT / Sensors', brand:'Hue',    supplier:'V1', cost:39, price:39, unit:'ea', min:2, max:6, qty:4, allocated:1, barcode:'046677473068', loc:[{l:'L6',b:'P3',q:4}], variants:null, lots:null, tags:[], updated:'2026-04-19', img:'#10b981' },

  // — Tools —
  { id:'I060', sku:'TOOL-CRMP-RJ',  name:'Cat6 Crimp Tool',         cat:'Tools', brand:'Klein',     supplier:'V1', cost:29, price:29, unit:'ea', min:1, max:1, qty:1, allocated:0, barcode:'092644721472', loc:[{l:'L7',b:'D2',q:1}], variants:null, lots:null, tags:[], updated:'2026-01-08', img:'#64748b' },
  { id:'I061', sku:'TOOL-MULTI',    name:'Fluke 117 Multimeter',    cat:'Tools', brand:'Fluke',     supplier:'V7', cost:189, price:189, unit:'ea', min:1, max:1, qty:1, allocated:0, barcode:'095969276009', loc:[{l:'L7',b:'D2',q:1,serial:['SN-FL-117-220183']}], variants:null, lots:null, tags:['calibrated'], updated:'2026-03-30', img:'#64748b' },
  { id:'I062', sku:'TOOL-PUNCH',    name:'Krone Punch Down',        cat:'Tools', brand:'Klein',     supplier:'V1', cost:24, price:24, unit:'ea', min:1, max:1, qty:1, allocated:0, barcode:'092644512063', loc:[{l:'L7',b:'D2',q:1}], variants:null, lots:null, tags:[], updated:'2026-01-08', img:'#64748b' },
  { id:'I063', sku:'TOOL-IRON',     name:'Pinecil Soldering Iron',  cat:'Tools', brand:'Pine64',    supplier:'V8', cost:30, price:30, unit:'ea', min:1, max:1, qty:1, allocated:0, barcode:'192876193418', loc:[{l:'L7',b:'D3',q:1,serial:['SN-PINECIL-V2']}], variants:null, lots:null, tags:[], updated:'2026-02-20', img:'#64748b' },

  // — Consumables —
  { id:'I070', sku:'CON-SOLDER-63', name:'Solder · 63/37 Lead 0.6mm',cat:'Consumables', brand:'Kester', supplier:'V7', cost:24, price:24, unit:'roll', min:1, max:3, qty:2, allocated:0, barcode:'737880246109', loc:[{l:'L7',b:'D3',q:2}], variants:null, lots:[{lot:'KS-2025-Q4',exp:null,q:2}], tags:[], updated:'2026-03-12', img:'#a16207' },
  { id:'I071', sku:'CON-PASTE-TIM', name:'Thermal Paste · MX-6',    cat:'Consumables', brand:'Arctic', supplier:'V1', cost:10, price:10, unit:'tube', min:2, max:6, qty:3, allocated:0, barcode:'872767007123', loc:[{l:'L6',b:'P4',q:3}], variants:null, lots:[{lot:'ARC-A2241',exp:'2028-09-01',q:3}], tags:[], updated:'2026-04-04', img:'#a16207' },
  { id:'I072', sku:'CON-ZIP-100',   name:'Zip Ties · 6" (100pk)',   cat:'Consumables', brand:'Generic', supplier:'V1', cost:5, price:5, unit:'pk', min:2, max:6, qty:5, allocated:0, barcode:'723175311003', loc:[{l:'L6',b:'P4',q:5}], variants:[{name:'Black',q:3},{name:'Natural',q:2}], lots:null, tags:[], updated:'2026-04-05', img:'#a16207' },
  { id:'I073', sku:'CON-ISOPRO-1', name:'Isopropyl 99% · 1L',      cat:'Consumables', brand:'MG Chem',supplier:'V7', cost:18, price:18, unit:'L', min:1, max:3, qty:1, allocated:0, barcode:'770125482410', loc:[{l:'L6',b:'P4',q:1}], variants:null, lots:[{lot:'MG-2025-A',exp:'2027-06-30',q:1}], tags:['low-stock','flammable'], updated:'2026-04-22', img:'#a16207' },
  { id:'I074', sku:'CON-LBL-BR',    name:'Brother PT Label · TZe-231', cat:'Consumables', brand:'Brother', supplier:'V1', cost:14, price:14, unit:'roll', min:1, max:4, qty:3, allocated:0, barcode:'012502065463', loc:[{l:'L6',b:'P4',q:3}], variants:null, lots:null, tags:[], updated:'2026-03-18', img:'#a16207' },

  // — Spare parts —
  { id:'I080', sku:'SPR-FAN-120',   name:'Noctua NF-A12x25 PWM',    cat:'Spare parts', brand:'Noctua', supplier:'V1', cost:33, price:33, unit:'ea', min:2, max:6, qty:5, allocated:0, barcode:'842431012010', loc:[{l:'L6',b:'P1',q:5}], variants:null, lots:null, tags:[], updated:'2026-04-01', img:'#9333ea' },
  { id:'I081', sku:'SPR-PSU-650',   name:'Corsair RM650x PSU',      cat:'Spare parts', brand:'Corsair', supplier:'V1', cost:119, price:119, unit:'ea', min:1, max:2, qty:1, allocated:0, barcode:'843591078900', loc:[{l:'L8',b:'SH1',q:1,serial:['SN-RM650X-Q9X']}], variants:null, lots:null, tags:[], updated:'2026-02-10', img:'#9333ea' },
  { id:'I082', sku:'SPR-RAM-32E',   name:'DDR4 ECC 32GB 3200',      cat:'Spare parts', brand:'Crucial', supplier:'V8', cost:79, price:79, unit:'ea', min:2, max:8, qty:4, allocated:0, barcode:'649528829221', loc:[{l:'L8',b:'SH1',q:4,serial:['SN-RAM-A','SN-RAM-B','SN-RAM-C','SN-RAM-D']}], variants:null, lots:null, tags:['ecc'], updated:'2026-03-25', img:'#9333ea' },

  // — Media —
  { id:'I090', sku:'MED-USB-128',   name:'USB 3.2 Drive · 128GB',   cat:'Media', brand:'Samsung',   supplier:'V1', cost:18, price:18, unit:'ea', min:3, max:8, qty:5, allocated:0, barcode:'887276590103', loc:[{l:'L6',b:'P1',q:5}], variants:null, lots:null, tags:['bootable'], updated:'2026-04-08', img:'#0891b2' },
];

// Activity log — most recent first
const ACTIVITY = [
  { ts:'2026-05-05 14:32', user:'me', type:'receive',  ref:'PO-1041', desc:'Received 4× UniFi U6 AP from Ubiquiti Store' },
  { ts:'2026-05-05 13:18', user:'me', type:'pick',     ref:'SO-220',  desc:'Picked 2× Cat6A 7ft Blue for project: NAS rebuild' },
  { ts:'2026-05-05 11:02', user:'me', type:'count',    ref:'CYC-08',  desc:'Cycle count · Cable Drawer · 3 discrepancies adjusted' },
  { ts:'2026-05-05 09:44', user:'me', type:'transfer', ref:'TR-78',   desc:'Moved 1× ESP32-S3 from Spare Parts → Lab Bench' },
  { ts:'2026-05-04 22:15', user:'me', type:'adjust',   ref:'ADJ-30',  desc:'Damaged: 1× Cat6A 3ft removed (cable jacket nicked)' },
  { ts:'2026-05-04 19:00', user:'me', type:'create',   ref:'I074',    desc:'Created item: Brother PT Label · TZe-231' },
  { ts:'2026-05-04 16:33', user:'me', type:'receive',  ref:'PO-1040', desc:'Received 1× Pinecil V2 + 2× Solder roll' },
  { ts:'2026-05-04 12:09', user:'me', type:'serial',   ref:'SN-USW24-A1F0',desc:'Assigned serial to UniFi Switch Pro 24 → RACK-A · U6' },
  { ts:'2026-05-03 18:22', user:'me', type:'reorder',  ref:'PO-1042', desc:'Auto-PO drafted: 4× Cat6A 3ft (below min)' },
  { ts:'2026-05-03 14:51', user:'me', type:'pick',     ref:'SO-219',  desc:'Picked 1× WD Red Pro 18TB for: array expansion' },
  { ts:'2026-05-03 10:00', user:'me', type:'audit',    ref:'AUD-12',  desc:'Quarterly audit started · Main Rack' },
  { ts:'2026-05-02 21:40', user:'me', type:'lot',      ref:'L26-04',  desc:'Lot tracked: 14× AAA Battery exp 2030-12-31' },
];

// Purchase orders
const PURCHASE_ORDERS = [
  { id:'PO-1042', supplier:'V3', status:'draft',     created:'2026-05-03', expected:'2026-05-10', total: 80, lines:[{sku:'CBL-CAT6A-3',qty:8,cost:5},{sku:'CBL-DAC-3M',qty:2,cost:25}] },
  { id:'PO-1041', supplier:'V4', status:'received',  created:'2026-04-28', expected:'2026-05-05', total:1116, received:'2026-05-05', lines:[{sku:'NET-UAP-U6E',qty:4,cost:279}] },
  { id:'PO-1040', supplier:'V7', status:'received',  created:'2026-04-30', expected:'2026-05-04', total: 78, received:'2026-05-04', lines:[{sku:'TOOL-IRON',qty:1,cost:30},{sku:'CON-SOLDER-63',qty:2,cost:24}] },
  { id:'PO-1039', supplier:'V1', status:'in-transit',created:'2026-05-02', expected:'2026-05-07', total:178, lines:[{sku:'STR-MX-2T-NV',qty:1,cost:179}] },
  { id:'PO-1038', supplier:'V6', status:'ordered',   created:'2026-05-04', expected:'2026-05-08', total: 60, lines:[{sku:'IOT-ESP32',qty:5,cost:12}] },
  { id:'PO-1037', supplier:'V2', status:'received',  created:'2026-04-15', expected:'2026-04-22', total:200, received:'2026-04-22', lines:[{sku:'NET-MS-10G',qty:4,cost:35},{sku:'NET-GBIC-LR',qty:1,cost:48}] },
  { id:'PO-1036', supplier:'V8', status:'cancelled', created:'2026-04-10', expected:'2026-04-25', total:420, lines:[{sku:'SVR-NUC-I5',qty:1,cost:420}] },
];

// Sales / pick orders (project allocations in homelab context)
const SALES_ORDERS = [
  { id:'SO-221', proj:'Mesh Wi-Fi rollout', status:'open',     created:'2026-05-05', priority:'high',   lines:[{sku:'NET-UAP-U6E',qty:1},{sku:'CBL-CAT6A-7',qty:2}] },
  { id:'SO-220', proj:'NAS rebuild',         status:'picking',  created:'2026-05-05', priority:'medium', lines:[{sku:'CBL-CAT6A-7',qty:2}] },
  { id:'SO-219', proj:'Array expansion',     status:'shipped',  created:'2026-05-03', priority:'high',   lines:[{sku:'STR-WD18-RED',qty:1}] },
  { id:'SO-218', proj:'Sensor garden v2',    status:'open',     created:'2026-05-04', priority:'low',    lines:[{sku:'IOT-ZB-TEMP',qty:4},{sku:'IOT-MOT-PIR',qty:1}] },
  { id:'SO-217', proj:'Edge cluster',        status:'shipped',  created:'2026-05-01', priority:'medium', lines:[{sku:'SVR-PI5-8GB',qty:1}] },
];

// Stock transfers
const TRANSFERS = [
  { id:'TR-78', from:'L6', to:'L7', date:'2026-05-05', status:'done',     lines:[{sku:'IOT-ESP32',qty:1}] },
  { id:'TR-77', from:'L4', to:'L1', date:'2026-05-04', status:'done',     lines:[{sku:'NET-UAP-U6E',qty:1}] },
  { id:'TR-76', from:'L8', to:'L1', date:'2026-05-02', status:'done',     lines:[{sku:'STR-WD18-RED',qty:2}] },
  { id:'TR-75', from:'L4', to:'L2', date:'2026-05-06', status:'pending',  lines:[{sku:'NET-MS-10G',qty:2}] },
];

// Cycle counts / audits
const COUNTS = [
  { id:'CYC-08', loc:'L5', date:'2026-05-05', status:'done',    counted:6,  variance:3, by:'me' },
  { id:'CYC-07', loc:'L4', date:'2026-04-28', status:'done',    counted:14, variance:0, by:'me' },
  { id:'CYC-06', loc:'L1', date:'2026-04-15', status:'done',    counted:18, variance:1, by:'me' },
  { id:'AUD-12', loc:'L1', date:'2026-05-03', status:'open',    counted:5,  variance:0, by:'me' },
  { id:'CYC-09', loc:'L6', date:'2026-05-08', status:'scheduled',counted:0, variance:0, by:'me' },
];

// 30-day stock-on-hand history (small, deterministic synthetic series)
const SOH_30D = (() => {
  const base = 132;
  const arr = [];
  for (let i = 0; i < 30; i++) {
    const drift = Math.sin(i * 0.6) * 6 + Math.cos(i * 0.31) * 4 + (i / 10);
    const noise = ((i * 7919) % 11) - 5;
    arr.push(Math.round(base + drift + noise));
  }
  return arr;
})();

// Compute a few ops metrics synthetically
const STATUS = {
  totalSKUs: ITEMS.length,
  totalUnits: ITEMS.reduce((s, i) => s + i.qty, 0),
  totalValue: Math.round(ITEMS.reduce((s, i) => s + i.qty * i.cost, 0)),
  lowStock:  ITEMS.filter(i => i.qty > 0 && i.qty < i.min).length,
  outOfStock: ITEMS.filter(i => i.qty === 0).length,
  serializedUnits: ITEMS.reduce((s, i) => s + i.loc.reduce((a,l)=>a+(l.serial?.length||0),0), 0),
  openPOs:   PURCHASE_ORDERS.filter(p => p.status !== 'received' && p.status !== 'cancelled').length,
  openSOs:   SALES_ORDERS.filter(s => s.status === 'open' || s.status === 'picking').length,
  pendingTransfers: TRANSFERS.filter(t => t.status === 'pending').length,
  scheduledCounts: COUNTS.filter(c => c.status === 'scheduled' || c.status === 'open').length,
};

// Prefer the snapshot the Rust backend injects during server-side
// hydration; fall back to the bundled seed dataset above so the
// prototype still works when loaded as a static file with no backend.
(function hydrate() {
  const boot = window.__RACKLOG_BOOTSTRAP__;
  if (!boot) {
    Object.assign(window, {
      LOCATIONS, SUPPLIERS, CATEGORIES, ITEMS, ACTIVITY,
      PURCHASE_ORDERS, SALES_ORDERS, TRANSFERS, COUNTS, SOH_30D, STATUS,
    });
    return;
  }
  Object.assign(window, {
    LOCATIONS:       boot.locations       || LOCATIONS,
    SUPPLIERS:       boot.suppliers       || SUPPLIERS,
    CATEGORIES,
    ITEMS:           boot.items           || ITEMS,
    ACTIVITY:        boot.activity        || ACTIVITY,
    PURCHASE_ORDERS: boot.purchaseOrders  || PURCHASE_ORDERS,
    SALES_ORDERS:    boot.salesOrders     || SALES_ORDERS,
    TRANSFERS:       boot.transfers       || TRANSFERS,
    COUNTS:          boot.counts          || COUNTS,
    SOH_30D,
    STATUS:          boot.status          || STATUS,
  });
})();
