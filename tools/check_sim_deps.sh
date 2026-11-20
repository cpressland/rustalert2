#!/bin/sh
# Fail if ra2-sim has a direct dependency not in crates/ra2-sim/allowed-deps.txt.
# The sim must stay deterministic: no I/O, clocks, threads or floating point.
set -eu
cd "$(dirname "$0")/.."
allowed=crates/ra2-sim/allowed-deps.txt
deps=$(cargo tree -p ra2-sim -e normal,build --depth 1 --prefix none --format '{p}' \
  | awk 'NR > 1 { print $1 }' | sort -u)
bad=0
for d in $deps; do
  if ! grep -qx "$d" "$allowed"; then
    echo "ra2-sim depends on $d, which is not in $allowed" >&2
    bad=1
  fi
done
[ "$bad" = 0 ] && echo "ra2-sim dependencies: ok ($(echo "$deps" | tr '\n' ' '))"
exit "$bad"
