# Firmware regression examples

These fixtures model the failure modes described in the
[main README](../../README.md#firmware-failures-you-can-exercise). Their
timing and state are synthetic. Use the target's own scheduler, hardware or
controller tests for product decisions.

The demo uses two kinds of Nool gate. If Nool is new to you, read
[Nool basics](../../docs/GUIDE.md#nool-basics) first.

- **Before landing.** Nool runs the test on the author's machine during
  `nool propose`. A failing test refuses the change, so it never enters
  history.
- **Before push.** The change can land locally, but `nool push` refuses to
  send it until a trusted runner, such as CI, has tested that exact knot and
  signed a pass (an *attestation*).

| Test | Failure it detects | Nool gate in the demo |
|---|---|---|
| `priority_inversion` | Six ticks of unrelated medium-priority work delay a low-priority lock holder. Without priority inheritance, the high-priority task waits eight ticks against a deadline of three. | Before push: a change to `priority_inversion.c` needs a signed `scheduler-deadline` pass. |
| `counter_rollover` | A 32-bit clock wraps before the heartbeat timeout. The broken version enters failsafe early. The fixed version computes elapsed time modulo the clock period and still expires at the real timeout. | Before landing: the counter test runs as a blocking check (`[[reification.checks]]` in `nool.toml`). |
| `uart_tx` | Servicing TX only after the shift register drains leaves an idle tick between bytes. Servicing the data register while the previous byte shifts keeps the wire busy. | Before push: a change to `uart_tx.c` needs a signed `uart-timing` pass. |

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

In addition to the host dependencies, you need Nool 8.0.2 or later, Git,
Bash, Python 3, OpenSSL with Ed25519 support (OpenSSL 1.1.1 or later; the
LibreSSL that ships with macOS does not support it, so use Homebrew's
`openssl`), and `xxd`. It takes about 20 seconds.

```sh
examples/firmware-regressions/verify-nool.sh
```

The script plays two people, an *author* who writes changes and a *runner*
(standing in for CI) who tests and signs them. Each has its own temporary
checkout, and they share a local Git remote and a disposable signing key.
Nothing outside the temporary directories is touched. It runs both the
broken and the fixed version of each test and checks the following:

- The broken counter change is refused at propose and never lands.
- The broken scheduling and UART changes land locally, but `nool push` holds
  them, both while no signed result exists and after the runner signs a
  `fail`.
- The runner tests exactly the knot the author landed. Publishing the
  runner's signed verdicts doesn't change the shared branch. Once both
  passes reach the author, `nool push` releases the tested change.

The script ends with a success message and exit code 0. Any other exit code
means one of these checks failed, and the lines above it say which.

The script configures the gates in its disposable repositories. For your
own repository, adapt the paths, trust a real runner's public key, and make
the runner test the exact revision it signs. [Validation and signed evidence setup](../../docs/GUIDE.md#reification-and-tla)
