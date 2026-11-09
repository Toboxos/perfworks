#!/usr/bin/env bash
# Export the starter project to STEP and round-trip it through FreeCAD's OCC kernel.
# See tools/validate_step.py for why this is not part of `cargo test`.
set -euo pipefail

FREECAD="${FREECAD:-/Applications/FreeCAD.app/Contents/Resources/bin/freecadcmd}"
STEP="${1:-${TMPDIR:-/tmp}/perfwork-validate.step}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ ! -x "$FREECAD" ]; then
  echo "freecadcmd not found at $FREECAD; set FREECAD=/path/to/freecadcmd" >&2
  exit 127
fi

PERFWORK_DUMP_STEP="$STEP" cargo test --manifest-path "$ROOT/Cargo.toml" dump_step >/dev/null
echo "wrote $STEP"
PERFWORK_STEP="$STEP" "$FREECAD" -c "exec(open('$ROOT/tools/validate_step.py').read())"
