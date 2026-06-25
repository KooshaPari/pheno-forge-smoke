#!/usr/bin/env bash
# Run pheno-forge-smoke against the live `pheno-cdylib-bridge` artifact.
#
# Usage:  ./scripts/run-smoke.sh [mode]
#   mode: mock (default) | sidecar | c

set -euo pipefail

MODE="${1:-mock}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE_ROOT="${BRIDGE_ROOT:-$REPO_ROOT/../pheno-cdylib-bridge}"

# 1. Find the bridge shared library
BRIDGE_LIB=""
for candidate in \
    "$BRIDGE_ROOT/target/release/libpheno_bridge.dylib" \
    "$BRIDGE_ROOT/target/release/libpheno_bridge.so" \
    "$BRIDGE_ROOT/target/release/libpheno_bridge.dll"; do
    if [[ -f "$candidate" ]]; then
        BRIDGE_LIB="$candidate"
        break
    fi
done

if [[ -z "$BRIDGE_LIB" ]]; then
    echo "ERROR: libpheno_bridge not found at $BRIDGE_ROOT/target/release/" >&2
    echo "       Run: cd $BRIDGE_ROOT && cargo build --release" >&2
    exit 2
fi

# 2. Build the smoke binary if it's missing
SMOKE_BIN="$REPO_ROOT/target/release/pheno-forge-smoke"
if [[ ! -x "$SMOKE_BIN" ]]; then
    echo "Building pheno-forge-smoke..."
    ( cd "$REPO_ROOT" && cargo build --release )
fi

# 3. Set the library path so the binary can dlopen the bridge
export PHENO_BRIDGE_LIB="$BRIDGE_LIB"
case "$(uname -s)" in
    Darwin)
        export DYLD_LIBRARY_PATH="$BRIDGE_ROOT/target/release:${DYLD_LIBRARY_PATH:-}"
        export DYLD_FALLBACK_LIBRARY_PATH="$BRIDGE_ROOT/target/release:${DYLD_FALLBACK_LIBRARY_PATH:-}"
        ;;
    Linux)
        export LD_LIBRARY_PATH="$BRIDGE_ROOT/target/release:${LD_LIBRARY_PATH:-}"
        ;;
    MINGW* | CYGWIN* | MSYS*)
        export PATH="$BRIDGE_ROOT/target/release:${PATH}"
        ;;
esac

echo "Bridge:   $BRIDGE_LIB"
echo "Smoke:    $SMOKE_BIN"
echo "Mode:     $MODE"
echo

exec "$SMOKE_BIN" --mode "$MODE"