#!/usr/bin/env bash
set -euo pipefail

example_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/embedded-regressions.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT
umask 077
unset NOOL_IDENTITY_KEY

openssl genpkey -algorithm ED25519 -out "$scratch/checker.pem"
public_key="$(openssl pkey -in "$scratch/checker.pem" -pubout -outform DER | tail -c 32 | xxd -p -c 64)"
private_seed="$(openssl pkey -in "$scratch/checker.pem" -outform DER | tail -c 32 | xxd -p -c 64)"

run() {
    if ! "$@" > "$scratch/command.log" 2>&1; then
        cat "$scratch/command.log" >&2
        return 1
    fi
}

expect_refusal() {
    local message="$1"
    shift
    if "$@" > "$scratch/command.log" 2>&1 || ! grep -Fq "$message" "$scratch/command.log"; then
        cat "$scratch/command.log" >&2
        printf 'Expected a refusal containing %s\n' "$message" >&2
        return 1
    fi
}

disable_fix() {
    sed "s/^#define $2 1$/#define $2 0/" "$example_dir/$1" > "$1"
}

configure_gates() {
    python3 - "$public_key" <<'PY'
from pathlib import Path
import sys

config = Path("nool.toml")
text = config.read_text()
for old, new in (
    ("checker_keys = []", f'checker_keys = ["ed25519:{sys.argv[1]}"]'),
    ('enforce = "warn"', 'enforce = "block"'),
    ("[attestation.obligations]", """[attestation.obligations]
"priority_inversion.c" = ["scheduler-deadline"]
"uart_tx.c" = ["uart-timing"]"""),
):
    if text.count(old) != 1:
        raise SystemExit(f"Unexpected nool init config: expected exactly one {old}")
    text = text.replace(old, new, 1)

config.write_text(text + """
[[reification.checks]]
name = "counter-rollover"
command = ["sh", "-c", "cmake -S . -B .nool/counter-build && cmake --build .nool/counter-build --target counter_rollover --clean-first && ctest --test-dir .nool/counter-build -R '^counter_rollover$' --output-on-failure --no-tests=error"]
extensions = ["c"]
pass_files = false
mode = "block"
""")
PY
}

record_result() {
    local test_name="$1" assertion="$2" expected_failure="$3" verdict
    if ctest --test-dir "$trial/build" -R "^$test_name$" --output-on-failure --no-tests=error > "$scratch/test.log" 2>&1; then
        verdict=pass
    else
        grep -Fq "$expected_failure" "$scratch/test.log" || {
            cat "$scratch/test.log" >&2
            return 1
        }
        verdict=fail
    fi
    if [[ "$verdict" != "$expected_verdict" ]]; then
        cat "$scratch/test.log" >&2
        printf 'Expected %s for %s but got %s\n' "$expected_verdict" "$test_name" "$verdict" >&2
        return 1
    fi
    NOOL_IDENTITY_KEY="$private_seed" run nool attest record --target "$knot" \
        --commit "$examined_commit" --checker demo-host-runner --assertion "$assertion" \
        --verdict "$verdict" --scope check --push origin
}

exercise() (
    expected_verdict="$1"
    trial="$scratch/$expected_verdict"
    export NOOL_SESSION_ID="embedded-regressions-$$-$expected_verdict-author"
    mkdir -p "$trial/author"
    git init --bare --quiet -b main "$trial/shared.git"
    cd "$trial/author"
    git init --quiet -b main
    run nool init --skip-discovery
    cp "$example_dir"/CMakeLists.txt "$example_dir"/*.c .

    if [[ "$expected_verdict" = pass ]]; then
        disable_fix priority_inversion.c PRIORITY_INHERITANCE
        disable_fix uart_tx.c REFILL_ON_TX_EMPTY
    fi
    run nool propose --path CMakeLists.txt priority_inversion.c counter_rollover.c uart_tx.c \
        --intent "Add the regression fixtures" --why "Set the starting revision" --solidify
    git remote add origin "$trial/shared.git"
    run nool push origin

    configure_gates
    run nool code trust
    run nool propose --path nool.toml --kind config --intent "Enable regression gates" \
        --why "Demonstrate local validation and signed push evidence" \
        --justification "Limit this disposable review gate to the two named demo source files" --solidify
    run nool code trust
    run nool push origin
    baseline="$(git rev-parse HEAD)"

    if [[ "$expected_verdict" = fail ]]; then
        disable_fix counter_rollover.c WRAP_SAFE_COUNTER
        expect_refusal "Lint check 'counter-rollover' failed" nool propose --path counter_rollover.c \
            --intent "Introduce a rollover regression" --why "Prove that CTest blocks landing" --solidify
        grep -Fq 'premature failsafe across rollover' "$scratch/command.log"
        test "$(git rev-parse HEAD)" = "$baseline"
        printf 'PASS counter rollover regression was refused before landing\n'
        cp "$example_dir/counter_rollover.c" .
        disable_fix priority_inversion.c PRIORITY_INHERITANCE
        disable_fix uart_tx.c REFILL_ON_TX_EMPTY
    else
        cp "$example_dir/priority_inversion.c" "$example_dir/uart_tx.c" .
    fi

    run nool propose --path priority_inversion.c uart_tx.c --intent "Change scheduling and UART behaviour" \
        --why "Exercise the signed evidence gate on the exact revision" --solidify
    examined_commit="$(git rev-parse HEAD)"
    knot="$(nool log --limit 1 --json | python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["id"])')"

    expect_refusal "scheduler-deadline missing" nool push origin
    grep -Fq 'uart-timing missing' "$scratch/command.log"
    test "$(git --git-dir="$trial/shared.git" rev-parse refs/heads/main)" = "$baseline"

    git clone --quiet "$trial/shared.git" "$trial/runner"
    (
        export NOOL_SESSION_ID="embedded-regressions-$$-$expected_verdict-runner"
        cd "$trial/runner"
        run nool init --skip-discovery
        run nool pull "$trial/author"
        test "$(git rev-parse HEAD)" = "$examined_commit"
        git diff --quiet HEAD
        run cmake -S . -B "$trial/build"
        run cmake --build "$trial/build"
        record_result priority_inversion scheduler-deadline "lock deadline missed"
        record_result uart_tx uart-timing "inter-byte gap exceeds"
    )
    test "$(git --git-dir="$trial/shared.git" rev-parse refs/heads/main)" = "$baseline"

    if [[ "$expected_verdict" = fail ]]; then
        run nool pull origin
        run nool attest obligations "$knot"
        grep -Eq 'scheduler-deadline[[:space:]]+fail' "$scratch/command.log"
        grep -Eq 'uart-timing[[:space:]]+fail' "$scratch/command.log"
        expect_refusal "proof obligation(s) unmet" nool push origin
        test "$(git --git-dir="$trial/shared.git" rev-parse refs/heads/main)" = "$baseline"
        printf 'PASS trusted failures kept both regressions off the shared branch\n'
    else
        for assertion in scheduler-deadline uart-timing; do
            run nool attest wait "$knot" --assertion "$assertion" --remote origin --timeout-secs 15
        done
        run nool push origin
        test "$(git --git-dir="$trial/shared.git" rev-parse refs/heads/main)" = "$examined_commit"
        printf 'PASS the two trusted passes released the exact tested revision\n'
    fi
)

exercise fail
exercise pass
