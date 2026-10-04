#!/bin/bash
# Regression checks for every bundled shader (golden pictures, orientation, text, coverage / flat blocks, temporal pops and
# brightness lurches, perf baselines), run with the `shaderlab` CLI, plus `cargo test` (which has the shader-specific camera,
# far-edge and long-run tests). Needs a GPU. Exits non-zero on any failure.
#   scripts/regress.sh            everything
#   scripts/regress.sh --fast     the quick subset (what the pre-push hook runs)
#   scripts/regress.sh --update   re-record goldens and perf baselines (review the diff, then commit)
set -euo pipefail
cd "$(dirname "$0")/.."
SHADERLAB=${SHADERLAB:-$(command -v shaderlab || true)}
if [[ -z "$SHADERLAB" && -x "$HOME/projects/shader-lab/target/release/shaderlab" ]]; then
  SHADERLAB="$HOME/projects/shader-lab/target/release/shaderlab"
fi
if [[ -z "$SHADERLAB" ]]; then
  echo "SKIPPED: shaderlab is not installed (cargo install --git https://github.com/evanspn/shader-lab); the golden/temporal/perf checks did not run"
  exit 0
fi
status=0
"$SHADERLAB" regress presets/shaders "$@" || status=$?
if [[ " $* " != *" --update "* ]]; then
  echo
  echo "cargo test:"
  cargo test --release --quiet 2>&1 | grep -E "test result|FAILED|SKIPPED|panicked" || true
  cargo test --release --quiet >/dev/null 2>&1 || status=1
fi
exit $status
