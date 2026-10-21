#!/usr/bin/env bash
# Downloads the Energy-Charts one-second grid frequency (RG Continental
# Europe, measured by Fraunhofer ISE in Freiburg) for 2024 in three-day
# windows, the API's limit per request, so a replay artifact can be
# compiled:
#
#   crates/bess-data/scripts/fetch-frequency.sh [dest-dir]
#   cargo run --release -p bess-data --bin compile-frequency -- <dest-dir> frequency-2024.bin
#   bess-emulator --frequency frequency-2024.bin ...
#
# Not bundled, not pinned: Energy-Charts data is free for non-commercial
# and journalistic use with attribution to "Energy-Charts / Fraunhofer
# ISE", which this repository's licences cannot carry. Running this script
# is your own acceptance of those terms, for your own use. The terms are
# recorded in DATA-LICENSES.md.
set -euo pipefail

DEST="${1:-energy-charts-raw}"
BASE="https://api.energy-charts.info/v2/frequency"
mkdir -p "$DEST"

day=0
count=0
while [ "$day" -lt 366 ]; do
  start=$(date -u -d "2024-01-01 +${day} days" +%Y-%m-%d 2>/dev/null \
    || date -u -v+${day}d -j -f %Y-%m-%d 2024-01-01 +%Y-%m-%d)
  end_day=$((day + 3))
  if [ "$end_day" -gt 366 ]; then end_day=366; fi
  end=$(date -u -d "2024-01-01 +${end_day} days" +%Y-%m-%d 2>/dev/null \
    || date -u -v+${end_day}d -j -f %Y-%m-%d 2024-01-01 +%Y-%m-%d)
  curl -fsS -o "$DEST/frequency_${start}.json" \
    "$BASE?start=${start}T00:00:00Z&end=${end}T00:00:00Z"
  count=$((count + 1))
  day=$((day + 3))
done
echo "$count responses under $DEST (attribution: Energy-Charts / Fraunhofer ISE)"
