# Firmware regression examples

These fixtures model the failure modes described in the
[main README](../../README.md#firmware-failures-you-can-exercise). Their
timing and state are synthetic. Use the target's own scheduler, hardware or
controller tests for product decisions.

| Test | Failure it detects | Nool gate in the demo |
|---|---|---|
| `priority_inversion` | Six ticks of unrelated medium-priority work delay a low-priority lock holder. Without priority inheritance, the high-priority task waits eight ticks against a deadline of three. | Before push, require `scheduler-deadline` evidence for changes to `priority_inversion.c`. |
| `counter_rollover` | A 32-bit clock wraps before the heartbeat timeout. The broken version enters failsafe early. The fixed version computes elapsed time modulo the clock period and still expires at the real timeout. | Before landing, run the counter test as a blocking `reification.checks` command. |
| `uart_tx` | Servicing TX only after the shift register drains leaves an idle tick between bytes. Servicing the data register while the previous byte shifts keeps the wire busy. | Before push, require `uart-timing` evidence for changes to `uart_tx.c`. |

## Run the host tests

From the repository root, with CMake 3.21 or later and a C compiler:

```sh
build_dir=$(mktemp -d)
cmake -S examples/firmware-regressions -B "$build_dir"
cmake --build "$build_dir"
ctest --test-dir "$build_dir" --output-on-failure --no-tests=error
```

All three tests should pass. The UART test also checks byte order, completion
after the final byte drains, and that TX interrupts end disabled.

## Run the Nool gates end to end

Use Nool 8.0.0, Git, Bash, Python 3, OpenSSL with Ed25519 support, and `xxd`
in addition to the host dependencies.

```sh
examples/firmware-regressions/verify-nool.sh
```

The script uses temporary author and runner checkouts, local bare Git
remotes, and a disposable signing key. It exercises both failing and passing
versions. It verifies that the bad counter change never lands. For the other
two tests, it checks that missing evidence and trusted failures both keep
the shared branch unchanged. A separate checkpoint checks that the runner
tested exactly what the author landed, publishing its signed verdicts leaves
the branch unchanged, and fetching both passes releases the tested commit.

The script configures the gates in its disposable repositories. For your
own repository, adapt the paths, trust a real runner's public key, and make
the runner test the exact revision it signs. [Validation and signed evidence setup](../../docs/GUIDE.md#reification-and-tla)
