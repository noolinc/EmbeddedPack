#!/usr/bin/env bash
# Everything a change must pass before it is proposed.
#   tools/check.sh            all policies
#   tools/check.sh <name>...  unit tests, then build and end-to-end test the named policies
set -uo pipefail
cd "$(dirname "$0")/.."
echo "== unit tests (one per rule) and guide tables"
cargo test -p emb-rules --quiet 2>&1 | grep -E "test result|FAILED|panicked|out of date|^\| EMB" || true
[ "${PIPESTATUS[0]}" -eq 0 ] || { echo "unit tests failed: cargo test -p emb-rules"; exit 1; }
fails=0
if [ "$#" -eq 0 ]; then
  set -- $(ls policy-src)
  for t in tests/tools/*.sh; do
    if bash "$t" >/dev/null 2>&1; then echo "PASS tool $(basename "$t" .sh)"; else echo "FAIL tool $(basename "$t" .sh) (bash $t)"; fails=$((fails+1)); fi
  done
fi
for name in "$@"; do
  if ! nool admin plugin build "policy-src/$name" >/dev/null 2>&1; then
    echo "FAIL $name: build (nool admin plugin build policy-src/$name)"; fails=$((fails+1)); continue
  fi
  if [ ! -f "tests/e2e/$name.sh" ]; then
    echo "FAIL $name: no tests/e2e/$name.sh"; fails=$((fails+1)); continue
  fi
  if out="$(bash "tests/e2e/$name.sh" 2>&1)"; then
    echo "PASS $name ($(echo "$out" | grep -c '^ok ') end-to-end cases)"
  else
    echo "FAIL $name"; echo "$out" | grep -A5 '^FAIL' ; fails=$((fails+1))
  fi
done
[ "$fails" -eq 0 ] && echo "ALL CHECKS PASS" || echo "$fails policy check(s) failed"
exit "$fails"
