#!/usr/bin/env bash
# Build the browser package and prepare it for npm as "bess-emulator".
#
# wasm-pack derives the npm package name from the crate name (bess-wasm),
# but the npm package carries the product name, same as the cargo binary.
# This script builds, patches pkg/package.json, and stops short of
# publishing; run `npm publish` inside crates/bess-wasm/pkg yourself.
set -euo pipefail
cd "$(dirname "$0")/.."

wasm-pack build crates/bess-wasm --target web --release

PKG=crates/bess-wasm/pkg
# Licenses live at the repo root; ship them and the README with the package.
cp LICENSE-MIT LICENSE-APACHE README.md "$PKG/"
node - "$PKG/package.json" <<'EOF'
const fs = require("fs");
const path = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
pkg.name = "bess-emulator";
pkg.description =
  "Synthetic grid-scale battery plant emulator: deterministic simulation kernel and 3D site view in one WASM module";
pkg.homepage = "https://github.com/AlitalipSever/bess-emulator";
pkg.keywords = ["bess", "battery", "energy-storage", "simulation", "emulator", "wasm", "scada"];
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
console.log(`patched ${path}: ${pkg.name}@${pkg.version}`);
EOF

echo
echo "Package ready in $PKG. To publish:"
echo "  cd $PKG && npm publish --access public"
