#!/usr/bin/env bash
# Downloads the SMARD weekly blocks of the German day-ahead price (DE-LU,
# hourly) for the 2024 local year, so the bundled price artifact can be
# regenerated:
#
#   crates/bess-data/scripts/fetch-prices.sh [dest-dir]
#   cargo run -p bess-data --bin compile-prices -- <dest-dir> \
#       crates/bess-data/data/dayahead-de-lu-2024.bin
#
# SMARD serves the series as weekly blocks that start on Monday 00:00
# Europe/Berlin. 2024-01-01 is a Monday, so the local year is the blocks
# from FIRST_BLOCK_MS (2024-01-01 00:00 CET) to LAST_BLOCK_MS (2024-12-30
# 00:00 CET), the last one trimmed by the compiler. The block list comes
# from SMARD's own index; the two anchors are the only constants here. The
# reproducibility anchor is the committed artifact and its pinned hash, not
# these downloads.
#
# Data source: Bundesnetzagentur | SMARD.de, CC BY 4.0. Attribution and
# terms recorded in DATA-LICENSES.md.
set -euo pipefail

DEST="${1:-smard-raw}"
BASE="https://www.smard.de/app/chart_data"
FILTER="4169" # wholesale day-ahead price, bidding zone DE-LU
REGION="DE"
FIRST_BLOCK_MS=1704063600000
LAST_BLOCK_MS=1735513200000

mkdir -p "$DEST"
blocks=$(curl -fsS "$BASE/$FILTER/$REGION/index_hour.json" \
  | grep -o '[0-9]\{13\}' \
  | awk -v lo="$FIRST_BLOCK_MS" -v hi="$LAST_BLOCK_MS" '$1 >= lo && $1 <= hi')

count=0
for ms in $blocks; do
  name="${FILTER}_${REGION}_hour_${ms}.json"
  curl -fsS -o "$DEST/$name" "$BASE/$FILTER/$REGION/$name"
  count=$((count + 1))
done

if [ "$count" -ne 53 ]; then
  echo "expected 53 weekly blocks for the 2024 local year, got $count" >&2
  exit 1
fi
echo "$count weekly blocks ready under $DEST"
