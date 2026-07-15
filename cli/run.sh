#!/usr/bin/env bash
# ----------------------------------------------------------------------------
# truss-fem convenience launcher (Linux / macOS / Git Bash).
#
#   ./run.sh                    Build (release) then start the interactive
#                               shell (help, samples, load, solve, view, ...).
#   ./run.sh demo               Build then run a quick demo:
#                               solve + open the web viewer for a sample.
#   ./run.sh <args...>          Build then forward all args to truss-fem.
#                               e.g.  ./run.sh solve input/bridge_pratt_2d.txt
#                                     ./run.sh view input/building_tower_3d.txt --open
# ----------------------------------------------------------------------------
set -euo pipefail
cd "$(dirname "$0")"

# Pick the platform binary name.
BIN="target/release/truss-fem"
[ -f "target/release/truss-fem.exe" ] && BIN="target/release/truss-fem.exe"

echo "[truss-fem] Building (release)..."
cargo build --release

if [ "$#" -eq 0 ]; then
  "$BIN" shell
elif [ "$1" = "demo" ]; then
  echo "[truss-fem] Demo: solve + web viewer for input/bridge_pratt_2d.txt"
  "$BIN" solve input/bridge_pratt_2d.txt
  "$BIN" view  input/bridge_pratt_2d.txt --open
else
  "$BIN" "$@"
fi
