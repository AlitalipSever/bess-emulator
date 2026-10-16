#!/usr/bin/env bash
# Play every library case headless and hold it to its committed record.
# What CI runs on every pull request; a drifted record names its fields.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p bess-emulator
status=0
for case in scenarios/*.yaml; do
  record="${case%.yaml}.digest.json"
  if ! target/release/bess-emulator --scenario "$case" --assert "$record"; then
    status=1
  fi
done
exit $status
