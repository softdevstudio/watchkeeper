#!/usr/bin/env bash
# Runs every check and records the results under test/results/latest/<os>/.
#
#   rustfmt, clippy with warnings denied, all tests, then the app's
#   end-to-end self-test on this OS's web engine.
#
# git history keeps earlier runs. Every number on the site comes from
# these files.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Linux*) os=linux ;;
  Darwin*) os=macos ;;
  MINGW*|MSYS*|CYGWIN*) os=windows ;;
  *) os=other ;;
esac
out="test/results/latest/$os"
rm -rf "$out"
mkdir -p "$out"
log="$out/checks.log"

{
  echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "commit: $(git rev-parse --short HEAD 2>/dev/null || echo none)"
  echo "os: $os ($(uname -srm))"
  rustc --version
  cargo --version
  echo
} > "$log"

step() {
  echo "== $*" | tee -a "$log"
  "$@" 2>&1 | tee -a "$log"
}

step cargo fmt --all -- --check
step cargo clippy --locked --all-targets -- -D warnings
step cargo test --locked
step cargo build --locked -p watchkeeper

bin=target/debug/watchkeeper
[ "$os" = windows ] && bin=target/debug/watchkeeper.exe

echo "== selftest" | tee -a "$log"
if [ "$os" = linux ] && [ -z "${DISPLAY:-}" ]; then
  xvfb-run -a -s "-screen 0 1600x1000x24" "$bin" --selftest "$out" 2>&1 | grep -E '^(PASS|FAIL|selftest)' | tee -a "$log"
else
  "$bin" --selftest "$out" 2>&1 | grep -E '^(PASS|FAIL|selftest)' | tee -a "$log"
fi
status=${PIPESTATUS[0]}
echo "selftest exit code: $status" | tee -a "$log"
exit "$status"
