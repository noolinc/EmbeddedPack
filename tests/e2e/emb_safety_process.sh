#!/usr/bin/env bash
# emb_safety_process refuses unexplained suppressions, untraced safety code,
# disabled tests and malformed deviations, and holds a knot that carries a
# deviation at push until a trusted reviewer key approves it.
# The repository profile is ASIL B, so requirement traces are demanded.
source "$(dirname "$0")/lib.sh"
setup_repo emb_safety_process
REMOTE="$WORK.remote.git"
REVIEWER="$WORK.reviewer"
trap 'rm -rf "$WORK" "$REMOTE" "$REVIEWER"' EXIT
mkdir -p src tests

printf '/* @req SRS-1 */\nint f(int a)\n{\n    return a; // NOLINT\n}\n' > src/a.c
expect_refused "suppression without a deviation" src/a.c "EMB-PRC-001" "MISRA Compliance:2020 4.1" "src/a.c:4"

printf 'int g(int a)\n{\n    return a;\n}\n' > src/g.c
expect_refused "safety code without a requirement trace" src/g.c "EMB-PRC-002" "NPR 7150.2D 3.12.1"

printf 'void TEST_IGNORE(void);\nvoid test_g(void)\n{\n    TEST_IGNORE();\n}\n' > tests/test_g.c
expect_refused "skipped test" tests/test_g.c "EMB-PRC-003" "NPR 7150.2D 4.5.3"

printf '/* @req SRS-1 */\n/* EMB-DEVIATION(EMB-C-001): ok */\nint h(void)\n{\n    return 1;\n}\n' > src/h.c
expect_refused "deviation without a real reason" src/h.c "EMB-PRC-004" "not deviatable"

printf "Checks: '-*'\n" > .clang-tidy
expect_refused "clang-tidy with every check off" .clang-tidy "EMB-PRC-006"

printf '/* @req SRS-2 */\nint g(int a)\n{\n    return a;\n}\n' > src/g.c
expect_landed "traced code" src/g.c

printf '/* @req SRS-1 */\nint f(int a)\n{\n    /* EMB-DEVIATION(EMB-PRC-001): vendor macro expands to a cast, see DR-3 */\n    return a; // NOLINT\n}\n' > src/a.c
expect_landed "suppression under a justified deviation" src/a.c
KNOT="$(echo "$OUT" | grep -o 'Landed knot [0-9a-f]*' | awk '{print $3}')"

# Push: the deviation needs an independent approval.
git init -q --bare "$REMOTE" && git remote add origin "$REMOTE"
step() { if [ "$2" = 1 ]; then echo "ok   $1"; else echo "FAIL $1"; echo "$OUT" | tail -4; FAILS=$((FAILS+1)); fi; }

OUT="$(nool push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 3 ] && echo "$OUT" | grep -q "evidence pending" && echo "$OUT" | grep -q "EMB-PRC-005" && ok=1
step "push held: deviation has no approval" $ok

REC="$(nool attest record --target "$KNOT" --checker reviewer --assertion emb-deviation-review --verdict pass --environment local 2>&1)"
KEY="$(echo "$REC" | grep -o 'ed25519:[0-9a-f]*' | head -1)"
OUT="$(nool push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 3 ] && echo "$OUT" | grep -q "EMB-PRC-005" && ok=1
step "push held: approval from an untrusted key does not count" $ok

# The repository owner trusts the reviewer's key.
sed -i.bak "s|^checker_keys = \[\]|checker_keys = [\"$KEY\"]|" nool.toml && rm -f nool.toml.bak
OUT="$(nool propose --path nool.toml --intent "trust the reviewer key" --why "e2e" --justification "Trust the independent reviewer key for deviation approvals" --solidify 2>&1)"
OUT="$(nool push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 0 ] && echo "$OUT" | grep -q "Pushed" && ok=1
step "push goes through once a trusted key approved" $ok

# The reviewer works in their own clone. The held change is not on the
# remote, so they pull it from the author's repository, sign, and push only
# the approval; the author fetches it and pushes the change.
printf '/* @req SRS-2 */\nint g(int a)\n{\n    /* EMB-DEVIATION(EMB-PRC-001): second vendor macro, see DR-4 */\n    return a; // NOLINT\n}\n' > src/g.c
expect_landed "second deviation" src/g.c
KNOT2="$(echo "$OUT" | grep -o 'Landed knot [0-9a-f]*' | awk '{print $3}')"
OUT="$(nool push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 3 ] && ok=1
step "push held: second deviation" $ok

AUTHOR="$PWD"
git clone -q "$REMOTE" "$REVIEWER" && cd "$REVIEWER" && nool init >/dev/null 2>&1
OUT="$(nool pull "$AUTHOR" 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 0 ] && grep -q "second vendor macro" src/g.c && ok=1
step "reviewer pulls the held change from the author's repository" $ok
OUT="$(nool attest record --target "$KNOT2" --checker reviewer --assertion emb-deviation-review --verdict pass --environment local --push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 0 ] && ! git --git-dir="$REMOTE" log main -- src/g.c | grep -q "DR-4" && ! git --git-dir="$REMOTE" show main:src/g.c 2>/dev/null | grep -q "DR-4" && ok=1
step "reviewer pushes the approval only; the change is still not on the remote branch" $ok

cd "$AUTHOR"
OUT="$(nool attest wait "$KNOT2" --assertion emb-deviation-review --remote origin --timeout-secs 60 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 0 ] && echo "$OUT" | grep -q "trusted" && ok=1
step "author receives the approval from the remote" $ok
OUT="$(nool push origin 2>&1)"; RC=$?
ok=0; [ "$RC" -eq 0 ] && echo "$OUT" | grep -q "Pushed" && git --git-dir="$REMOTE" show main:src/g.c 2>/dev/null | grep -q "DR-4" && ok=1
step "push goes through after the reviewer's approval arrives" $ok
finish
