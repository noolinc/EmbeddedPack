# Shared by the end-to-end policy tests: a throwaway Nool repository with one
# policy installed, and helpers that run a real `nool propose` against it.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FAILS=0

CLEAN=()
cleanup() { [ "${#CLEAN[@]}" -gt 0 ] && rm -rf "${CLEAN[@]}"; }
trap cleanup EXIT

setup_repo() { # <policy name> [extra hardware-profile.toml text]
  local policy="$1" extra="${2:-}"
  [ -f "$ROOT/policy-src/$policy/$policy.wasm" ] || { echo "build first: tools/build-policies.sh $policy"; exit 1; }
  cd "$ROOT" || exit 1
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/emb-e2e-XXXXXX")"
  CLEAN+=("$WORK")
  cd "$WORK" || exit 1
  git init -q . && nool init >/dev/null 2>&1 || { echo "nool init failed"; exit 1; }
  # Enterprise builds refuse agent edits without a contract; harmless elsewhere.
  nool admin contract grant --role editor >/dev/null 2>&1 || true
  local src="$ROOT/policy-src/$policy/$policy.toml"
  if [ -n "$extra" ]; then
    # A variant built against the repository profile plus `extra`.
    mkdir -p "$WORK/.e2e"
    { cat "$ROOT/hardware-profile.toml"; printf '\n%s\n' "$extra"; } > "$WORK/.e2e/profile.toml"
    (cd "$ROOT" && EMB_PROFILE="$WORK/.e2e/profile.toml" nool admin plugin build "policy-src/$policy" --out "$WORK/.e2e/$policy.wasm" >/dev/null 2>&1) \
      || { echo "variant build of $policy failed"; exit 1; }
    src="$WORK/.e2e/$policy.wasm"
  fi
  nool admin plugin install "$src" --policy --version 0.1.0 >/dev/null 2>&1 \
    || { echo "install of $policy failed"; exit 1; }
}

propose() { # <path> [extra propose flags...]  -> sets OUT, RC
  local path="$1"; shift
  OUT="$(nool propose --path "$path" --intent "e2e $path" --why "policy end-to-end test" "$@" --solidify 2>&1)"; RC=$?
}

expect_refused() { # <name> <path> <needle>...
  local name="$1" path="$2"; shift 2
  propose "$path"
  local ok=1
  [ "$RC" -ne 0 ] || ok=0
  echo "$OUT" | grep -q "POLICY_DENY" || ok=0
  for needle in "$@"; do echo "$OUT" | grep -qF -- "$needle" || ok=0; done
  if [ "$ok" = 1 ]; then echo "ok   refused: $name"; else echo "FAIL refused: $name (rc=$RC)"; echo "$OUT" | tail -5; FAILS=$((FAILS+1)); fi
}

expect_landed() { # <name> <path>
  propose "$2"
  if [ "$RC" -eq 0 ] && echo "$OUT" | grep -q "Landed knot"; then echo "ok   landed:  $1"; else echo "FAIL landed: $1 (rc=$RC)"; echo "$OUT" | tail -5; FAILS=$((FAILS+1)); fi
}

finish() { [ "$FAILS" -eq 0 ] && echo "PASS" || { echo "$FAILS failure(s)"; exit 1; }; }
