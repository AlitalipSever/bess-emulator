#!/usr/bin/env bash
# Build the browser package and publish it to npm as "bess-emulator".
#
# One command on purpose. wasm-pack derives the npm package name from the
# crate name, so a freshly built pkg/ says "bess-wasm", and the rename to the
# product name happens here. Splitting build from publish once left an
# unpatched pkg/ lying around from a local build, and the wrong package got
# published under the wrong name. So this script owns the whole path: it
# clears the directory, builds, patches, checks its own work, and publishes.
#
# Local builds that are not going to npm should use their own output
# directory and leave pkg/ alone:
#   wasm-pack build crates/bess-wasm --target web --release --out-dir ../../target/wasm-local
set -euo pipefail
cd "$(dirname "$0")/.."

PKG=crates/bess-wasm/pkg
NAME=bess-emulator

# Never build on top of a previous run: a stale file is how the wrong thing
# gets shipped.
rm -rf "$PKG"
wasm-pack build crates/bess-wasm --target web --release

# Licenses live at the repo root; ship them and the README with the package.
cp LICENSE-MIT LICENSE-APACHE README.md "$PKG/"
node - "$PKG/package.json" "$NAME" <<'PATCH'
const fs = require("fs");
const [path, name] = process.argv.slice(2);
const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
pkg.name = name;
pkg.description =
  "Synthetic grid-scale battery plant emulator: deterministic simulation kernel and 3D site view in one WASM module";
pkg.homepage = "https://github.com/AlitalipSever/bess-emulator";
pkg.keywords = ["bess", "battery", "energy-storage", "simulation", "emulator", "wasm", "scada"];
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
console.log(`patched ${path}: ${pkg.name}@${pkg.version}`);
PATCH

# Check the patch took before anything leaves this machine.
PUBLISHED_NAME=$(node -p "require('./$PKG/package.json').name")
VERSION=$(node -p "require('./$PKG/package.json').version")
if [ "$PUBLISHED_NAME" != "$NAME" ]; then
  echo "refusing to publish: package name is '$PUBLISHED_NAME', expected '$NAME'" >&2
  exit 1
fi

echo
echo "About to publish $PUBLISHED_NAME@$VERSION to npm."
echo "npm will ask for your one-time password."
echo
( cd "$PKG" && npm publish --access public )
