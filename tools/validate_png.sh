#!/usr/bin/env bash
# Render every board's front and back to PNG through a real GL context.
#
# Offscreen rendering is the one export path the test suite cannot reach: it needs a GPU
# context, which only exists inside a running window. The app exports and quits when
# PERFWORKS_DUMP_PNG is set, so the whole path still gets exercised end to end.
#
# Unit tests cover the geometry (src/export/image.rs); this covers the GL plumbing. A
# uniformly-coloured output means the render failed even though nothing errored -- that is
# how the framebuffer binding bug presented.
set -euo pipefail

OUT="${1:-${TMPDIR:-/tmp}/perfworks-png}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$OUT"

PERFWORKS_DUMP_PNG="$OUT" cargo run --manifest-path "$ROOT/Cargo.toml" 2>&1 | grep PERFWORKS_DUMP_PNG || true

shopt -s nullglob
files=("$OUT"/*.png)
if [ ${#files[@]} -eq 0 ]; then
  echo "FAIL: no images written" >&2
  exit 1
fi
for f in "${files[@]}"; do
  echo "  $(basename "$f") $(wc -c < "$f") bytes"
done
echo "wrote ${#files[@]} image(s) to $OUT"
