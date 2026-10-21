#!/usr/bin/env bash
# The TSO's per-second aFRR request ("Sekündlicher SRL-Sollwert") is
# published by Netztransparenz under the German TSOs' copyright, through
# a web API that needs a registered client; this repository neither
# bundles it nor fetches it for you. What this script does is check the
# file you obtained and hand it to the compiler:
#
#   1. Register at https://api-portal.netztransparenz.de/ and download
#      the per-second SRL setpoint for 2024 (or export it from the
#      portal's "Sekündliche Daten" page) as a CSV of `timestamp;mw`
#      lines, UTC timestamps, megawatts positive to deliver.
#   2. crates/bess-data/scripts/fetch-afrr.sh <csv> [demand-mw] [out]
#
# which runs `compile-afrr` with the demand (default 2000 MW, the German
# aFRR demand the TSOs' 2026 requirements name) and prints the hash a
# replayed run's record will carry. Terms recorded in DATA-LICENSES.md.
set -euo pipefail

CSV="${1:?a CSV of timestamp;mw lines}"
DEMAND="${2:-2000}"
OUT="${3:-afrr-2024.bin}"
cd "$(dirname "$0")/../../.."
head -3 "$CSV"
cargo run --release -p bess-data --bin compile-afrr -- "$CSV" "$DEMAND" "$OUT"
