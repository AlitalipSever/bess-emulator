#!/usr/bin/env bash
# Downloads the raw DWD CDC hourly observation archives for station 03015
# (Lindenberg (Mark) observatory) and unpacks them, so the bundled weather
# artifact can be regenerated:
#
#   crates/bess-data/scripts/fetch-weather.sh [dest-dir]
#   cargo run -p bess-data --bin compile-weather -- <dest-dir> \
#       crates/bess-data/data/lindenberg-2024.bin
#
# DWD re-issues the historical archives periodically (the date range in the
# file name moves), so this script resolves the current file names from the
# directory index instead of hard-coding them. The reproducibility anchor is
# the committed artifact and its pinned hash, not these archives.
#
# Data source: Deutscher Wetterdienst, Climate Data Center. Attribution and
# terms recorded in DATA-LICENSES.md.
set -euo pipefail

DEST="${1:-dwd-raw}"
BASE="https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/hourly"
STATION="03015"

resolve() { # resolve <product-path> <product-code>
  curl -fsS "$BASE/$1/" \
    | grep -o "stundenwerte_$2_${STATION}_[^\"]*\.zip" \
    | sort -u | head -1
}

fetch() { # fetch <product-path> <product-code>
  local name
  name=$(resolve "$1" "$2")
  if [ -z "$name" ]; then
    echo "no $2 archive for station $STATION under $1" >&2
    exit 1
  fi
  echo "fetching $name"
  curl -fsS -o "$DEST/$name" "$BASE/$1/$name"
  mkdir -p "$DEST/${name%.zip}"
  unzip -oq "$DEST/$name" -d "$DEST/${name%.zip}"
}

mkdir -p "$DEST"
fetch "air_temperature/historical" "TU"   # 2 m temperature + relative humidity
fetch "precipitation/historical"   "RR"   # precipitation amount + form
fetch "wind/historical"            "FF"   # wind speed + direction
fetch "cloudiness/historical"      "N"    # total cloud cover, okta
fetch "solar"                      "ST"   # global irradiance (no historical split)

echo "raw archives ready under $DEST"
