# Camera-based cycle counting

RACKLOG accepts vision-derived counts via `POST /api/counts/vision`.
The actual model + camera pipeline lives outside this binary —
RACKLOG's job is to record the observations, diff them against the
catalog, and audit the result. Your companion service does the
classification.

## Endpoint contract

```
POST /api/counts/vision
Authorization: Bearer rl_<api-key>
Content-Type: application/json

{
  "location_id":   "L1",
  "bin":           "U6",                       // optional
  "model":         "yolov8n-rack-2026-04",     // optional, recorded for drift
  "evidence_url":  "http://camera-1.lan/snap/abc.jpg",  // optional
  "observations": [
    { "sku": "NET-USW-PRO-24", "qty": 1, "confidence": 0.94 },
    { "sku": "STR-WD18-RED",   "qty": 6, "confidence": 0.91 }
  ]
}
```

Required role: `operator` or `admin`. Issue a per-camera API key
under that role so each pipeline's writes are clearly attributed
in the audit log (`count.vision` rows record the calling
username).

Server response:

```json
{
  "count_id":     "CYC-V-1714000000000",
  "observations": 2,
  "matched_skus": 2,
  "total_delta":  -1
}
```

`matched_skus` is the count of observed SKUs that exist in the
catalog. `total_delta` is `Σ (observed_qty − catalog_qty)` across
matched SKUs — a one-shot signal for "is the catalog drifting?".
Unknown SKUs are silently skipped: vision *corroborates* the
catalog, it never creates new records.

## Companion service skeleton (Python + Ultralytics YOLO)

```python
# vision-companion.py — runs on the Pi with the camera.
import os, time, json, requests
from ultralytics import YOLO
from picamera2 import Picamera2

RACKLOG = os.environ["RACKLOG_URL"]   # e.g. http://racklog.lan:8080
KEY     = os.environ["RACKLOG_API_KEY"]
LOC     = os.environ["RACKLOG_LOCATION_ID"]   # e.g. L1
BIN     = os.environ.get("RACKLOG_BIN")       # optional

# Train once on your own item photos. We already store an `img`
# field per item that you can pull from /api/items.
model = YOLO("racklog-rack.pt")

cam = Picamera2()
cam.start()

while True:
    frame = cam.capture_array()
    results = model(frame, verbose=False)[0]

    # Aggregate detections by class label → SKU.
    counts = {}
    confs  = {}
    for box in results.boxes:
        sku = model.names[int(box.cls)]
        counts[sku] = counts.get(sku, 0) + 1
        confs[sku] = max(confs.get(sku, 0.0), float(box.conf))

    payload = {
        "location_id":  LOC,
        "bin":          BIN,
        "model":        f"yolov8n-rack-{model.train.epoch}",
        "observations": [
            {"sku": sku, "qty": qty, "confidence": confs[sku]}
            for sku, qty in counts.items()
        ],
    }
    r = requests.post(
        f"{RACKLOG}/api/counts/vision",
        headers={"Authorization": f"Bearer {KEY}"},
        json=payload,
        timeout=10,
    )
    r.raise_for_status()
    print(time.time(), r.json())
    time.sleep(60)   # one observation per minute
```

## Operator setup

1. Train YOLO on your inventory photos.
   - Pull `/api/items` to get `img` URLs.
   - Or photograph each SKU manually (~30 frames per class is
     plenty for a homelab with consistent lighting).
   - Use [Roboflow](https://roboflow.com/) or [LabelImg](https://github.com/HumanSignal/labelImg) for labelling.
2. Create a per-camera operator user + API key.
   ```sh
   curl -b cookies -X POST $RACKLOG/api/admin/users \
     -d '{"username":"camera-rack-a","password":"…","role":"operator"}'
   curl -b cookies -X POST $RACKLOG/api/admin/api_keys \
     -d '{"user_id":"u-…","label":"rack-a camera"}'
   ```
3. Run the companion service. Pi 5 + Camera Module 3 with a
   YOLO-N model handles ~5 FPS comfortably; a Pi Zero 2 W with
   YOLO-N int8 manages ~2 FPS.
4. Watch the audit log — `count.vision` rows show every observation
   batch with the per-SKU deltas. Reconcile mismatches via the
   existing `/api/items/:id` PUT (operator can write the
   corrected `qty`) or by raising a normal cycle-count for an
   operator to walk through.

## Recommended thresholds

- **Confidence floor** — most homelab models start around 0.85 on
  trained classes. Drop observations below 0.5 in your companion;
  RACKLOG records what you push so set the bar before you POST.
- **Sampling rate** — once per minute is plenty. The Walmart-scale
  ambient-IoT story streams continuously; a homelab cares about
  *change*, not *latency*. Push only when frames differ from the
  last batch.
- **Evidence retention** — `evidence_url` is recorded on the count
  row but RACKLOG does not host the image itself. Use a static
  HTTP path on the Pi or a write-once S3-compatible bucket; rotate
  on a 30-day schedule.

## Why no schema for new SKUs

Allowing vision to register unknown classes turns a misclassified
frame into an inventory mutation, which is also a self-XSS
liability when the resulting "name" gets rendered in the dashboard
unescaped. The auto-import flow from a barcode scan goes through
the existing `/api/lookup/{barcode}` + `/api/items` POST path
where a human approves the create. Cycle-counting and SKU
discovery are kept deliberately separate.
