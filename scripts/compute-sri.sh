#!/usr/bin/env bash
# compute-sri.sh — print SHA-384 SRI hashes for the third-party
# scripts RACKLOG.html / rack3d.js pull from a CDN. Run on deploy
# so the integrity attributes pin the exact bytes the browser
# fetched at build time.
#
# Usage:
#   scripts/compute-sri.sh
#
# Output is `<url>: <integrity-string>` per line, suitable for
# pasting into the corresponding <script integrity="…"> attribute
# (or, for rack3d.js, into the <link rel="modulepreload"
# integrity="…"> tag we add to RACKLOG.html alongside the dynamic
# import).

set -euo pipefail

URLS=(
  "https://unpkg.com/react@18.3.1/umd/react.development.js"
  "https://unpkg.com/react-dom@18.3.1/umd/react-dom.development.js"
  "https://unpkg.com/@babel/standalone@7.29.0/babel.min.js"
  "https://unpkg.com/three@0.169.0/build/three.module.min.js"
  "https://unpkg.com/three@0.169.0/examples/jsm/controls/OrbitControls.js"
)

for url in "${URLS[@]}"; do
  hash=$(curl -fsSL "$url" | openssl dgst -sha384 -binary | openssl base64 -A)
  printf '%s: sha384-%s\n' "$url" "$hash"
done
