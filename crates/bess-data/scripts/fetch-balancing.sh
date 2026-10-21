#!/usr/bin/env bash
# Downloads the German TSOs' 2024 balancing tender results from
# regelleistung.net's data centre, one file per day and product: the FCR
# capacity results (six four-hour blocks, settlement capacity price per
# country), the aFRR capacity results (positive and negative, six blocks)
# and the aFRR energy results (the merit order's minimum, average and
# marginal price per quarter-hour), so a balancing artifact can be
# compiled:
#
#   crates/bess-data/scripts/fetch-balancing.sh [dest-dir]
#   crates/bess-data/scripts/xlsx-to-csv.py <dest-dir>
#   cargo run --release -p bess-data --bin compile-balancing -- <dest-dir> balancing-2024.bin
#   bess-emulator --balancing balancing-2024.bin ...
#
# The platform serves xlsx only; the converter (standard-library Python)
# writes a CSV beside each file, which is what the compiler reads, so the
# data crate stays free of dependencies.
#
# Not bundled, not pinned: the platform publishes the results without a
# licence statement, so the files are yours, fetched under your own
# reading of the platform's terms; the emulator bundles only the annual
# figures CALIBRATION.md cites. Terms recorded in DATA-LICENSES.md.
set -euo pipefail

DEST="${1:-regelleistung-raw}"
BASE="https://www.regelleistung.net/apps/cpp-publisher/api/v1/download/tenders/resultsoverview"
mkdir -p "$DEST"

day_after() {
  # $1 days after 2024-01-01, as YYYY-MM-DD, on GNU and BSD date alike.
  date -u -d "2024-01-01 +$1 days" +%Y-%m-%d 2>/dev/null \
    || date -u -v+"$1"d -j -f %Y-%m-%d 2024-01-01 +%Y-%m-%d
}

count=0
for d in $(seq 0 365); do
  day=$(day_after "$d")
  for spec in "CAPACITY FCR" "CAPACITY aFRR" "ENERGY aFRR"; do
    market=${spec% *}
    product=${spec#* }
    out="$DEST/${product}_${market}_${day}.xlsx"
    if [ ! -s "$out" ]; then
      curl -fsS -o "$out" "$BASE?date=${day}&exportFormat=xlsx&market=${market}&productTypes=${product}"
    fi
    count=$((count + 1))
  done
done
echo "$count files under $DEST (source: regelleistung.net, the German TSOs' data centre)"
