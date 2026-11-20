# Embedded Policy Pack for Nool

**Firmware rules that are enforced when a change is made, not reported after it.**

MISRA, BARR-C, AUTOSAR C++14, Power of 10, ETSI EN 303 645, NIST SP 800-193:
every embedded team has the rules. The hard part is keeping them while the code
changes every day, under schedule pressure, and now with AI agents writing a
growing share of it. This pack turns 79 of those rules into gates that
[Nool](#why-nool) runs on every change, for every author, human or AI. A
change that breaks a rule does not land.

It works the same with or without AI. A team that writes every line by hand
gets strict, immediate, clause-level feedback during development. A team that
uses coding agents gets a boundary the agent cannot talk its way around.

## See it work

An engineer (or an agent) adds a diagnostic print to a UART interrupt handler:

```c
void USART1_IRQHandler(void)
{
    rx_count++;
    printf("rx %u\n", rx_count);
}
```

```console
$ nool propose --path src/uart.c --intent "Count received bytes in the UART interrupt" \
      --why "Diagnostics for dropped bytes" --solidify

Error: Gate stack denied [POLICY_DENY] at stage pre-propose:
[EMB-RT-001] src/uart.c:7: printf() in ISR USART1_IRQHandler() (an ISR shall not block,
allocate, print or call the kernel's task-level API) -- BARR-C:2018 6.5
  | deviate: /* EMB-DEVIATION(EMB-RT-001): <reason> */ on or above the line
profile: generic-cortex-m4 (RAM 65536 B, flash 262144 B, fpu Single, Rtos, heap None, ASIL B, 62304 class B, SIL 2)
```

The refusal says what, where, why, which clause, and for which hardware. The
author moves the print out of the handler and proposes again:

```console
✓ Landed knot 9279fcc7
```

Nothing reached the branch in between. No CI run, no review round, no ticket.

When a rule really has to be broken, the author writes a deviation beside the
code, and the change is held at push until a reviewer the repository trusts
signs it off:

```console
$ nool push origin
Error: push refused: evidence pending for Knot 20576728 (policy emb_safety_process:
[EMB-PRC-005] src/a.c:4 deviates EMB-PRC-001 (a deviation shall be authorized by an
independent reviewer before push) -- MISRA Compliance:2020 4.1
  | waiting for a trusted `emb-deviation-review` pass)
```

The author's own signature does not release it.

## What you get

| Without the pack | With the pack |
|---|---|
| Violations appear in a CI analyser report after the code is written | The change is refused as it is proposed, with the clause, while the author still has the context |
| Green can be bought with `NOLINT`, `-w`, a skipped test or an edited `.clang-tidy` | Each of those is itself a refused change |
| Tools do not know the board | Rules use your RAM, flash, FPU, heap policy, execution model and safety level |
| Deviations are tracked in a spreadsheet and approved by email | A deviation sits beside the code and needs a signed approval from a trusted key before push |
| An AI agent is trusted to follow the coding standard | The agent gets the same refusal as a person and corrects itself in its own loop |
| Audit evidence is assembled by hand | Every landed change and every approval is a signed record in the history |

## The eight policies

| Policy | Rules | Refuses | Sources |
|---|---|---|---|
| `emb_c_subset` | 19 | `goto`, `setjmp`, banned library functions, octal constants, missing `default`, fall-through, unbraced bodies, assignment in conditions | MISRA C:2012, BARR-C:2018, CERT C, Power of 10 |
| `emb_cpp_subset` | 18 | C-style casts, `reinterpret_cast`, `const_cast`, `NULL`, banned headers and functions, variadics, `union`, unscoped enums | AUTOSAR C++14, JSF AV C++ |
| `emb_resource_bounds` | 6 | Heap use beyond the profile, recursion, `alloca`/VLAs, unbounded loops, over-long functions, deep pointers | MISRA C:2012, Power of 10, JSF AV |
| `emb_hardware_fit` | 6 | Stack objects too big for the RAM, floating point without the FPU, flash-heavy libraries, exceptions without a heap, code that contradicts superloop / RTOS / event-driven | Your hardware profile |
| `emb_security_baseline` | 11 | Hard-coded credentials and private keys, weak crypto, `rand`, `gets`, unbounded string functions, disabled TLS verification, cleartext endpoints | ETSI EN 303 645, CWE |
| `emb_runtime_safety` | 5 | Blocking or kernel task-level calls in an ISR, interrupts left disabled, non-reentrant library calls, watchdog disabled or fed from an ISR | BARR-C:2018, NASA-GB-8719.13, CERT C |
| `emb_firmware_integrity` | 7 | Silenced warnings, removed hardening, secure boot or rollback protection off, development signing keys, unauthenticated build downloads, open debug access | NIST SP 800-193, SP 800-218, UN R156, EN 303 645 |
| `emb_safety_process` | 7 | Analysis suppressions, untraced safety code, disabled tests, malformed deviations, weakened analysis config; holds unapproved deviations at push | MISRA Compliance:2020, NASA NPR 7150.2D |

Every rule, its clause and what it cannot see: [docs/GUIDE.md](docs/GUIDE.md).

## Hardware-aware by design

Firmware architecture follows the hardware. Code that is correct on a 1 MB
Cortex-M7 with an RTOS is a defect on a 16 KB superloop part. You describe the
target once:

```toml
[hardware]
ram_bytes = 65536
flash_bytes = 262144
fpu = "single"           # none | single | double

[architecture]
execution_model = "rtos" # superloop | rtos | event-driven
heap = "none"            # none | init-only | allowed

[safety]
iso26262_asil = "B"      # QM | A | B | C | D
iec62304_class = "B"
iec61508_sil = 2
```

and the rules follow: `double` is refused on a single-precision FPU,
`vTaskDelay` on a superloop, a 2 KB stack buffer on 64 KB of RAM, `malloc`
with no heap. At ASIL C/D, class C or SIL 3/4 the strict rules can no longer
be deviated at all.

## Why Nool

Nool is a version-control system built for changes made by people and agents
together. Its policy engine is **Aram (Added Rules and Measures)**: every
change passes through a stack of policies, each one a WASM component running
in a safety sandbox, before it can become history. This pack is eight Aram
policies.

What Aram provides that the pack depends on:

- **A gate before history.** Policies run when a change is proposed, when it
  is sealed, and when it is pushed. A refused change never becomes a commit.
  Landed changes are mirrored to Git, so the rest of your toolchain keeps
  working.
- **A safety sandbox.** A policy has no file system and no network.
  It gets the change and returns a verdict, inside fixed fuel and memory
  limits with strict floating-point checks. The same change gets the same
  verdict on every machine, and a policy cannot leak or alter your source. If
  a policy fails to run, the change is blocked, not waved through.
- **A pinned, replicated rule set.** Each policy's hash is pinned in
  `nool.toml` and travels with the repository. Changing or removing a policy
  is itself a signed, justified change in the history. It cannot happen
  quietly on one laptop.
- **One gate for every author.** A person's change and an agent's change go
  through the same policies and get the same refusal text.
- **Signed evidence.** Changes and reviewer attestations are signed, and trust
  comes from keys listed in the repository. That is what makes deviation
  approval real.

The pack has eight policies. Nool's Community edition loads up to 3 policies
and Team up to 11; Enterprise has no limit. On Community, start with the three
that matter most to you.

## Why not a pre-commit hook or a linter?

Both are worth having, and neither can do this job. The difference is not the
rules; it is who is able to switch them off.

**A Git pre-commit hook is a request.**

| Pre-commit hook | Aram |
|---|---|
| Lives in each clone and is not installed by `git clone`; a new laptop or a new agent sandbox has no hook until someone sets it up | The pinned policy set arrives with the repository and runs wherever the repository is used |
| `git commit --no-verify` skips it, and an agent under pressure to finish will find that flag | There is no flag. The gate is part of how a change becomes history |
| Runs whatever tool versions that machine has, so two engineers get two answers | A sandboxed WASM component with a pinned hash gives one answer everywhere |
| Is a script with the author's full privileges: it can read keys, reach the network, or be edited to print "ok" | Runs with no file or network access |
| Leaves no proof that it ran | Each landed change is a signed record made under the pinned policy set |
| Cannot ask "has someone else approved this?" | Can hold a change at push until a trusted key has attested it |

Server-side hooks and CI close some of these gaps, but they act after the code
is written, committed and pushed. The author has moved on, the agent's session
has ended, and the fix becomes a second change and a second review.

**A linter is an opinion.**

| Linter or static analyser | Aram |
|---|---|
| Reports findings; whoever runs it decides what a finding means | Decides: the change lands or it does not |
| Is silenced by the author, in the same change, with `NOLINT`, a pragma, `-w` or an edit to its config | Those edits are changes too, and this pack refuses them (EMB-PRC-001, EMB-PRC-006, EMB-FWI-001) |
| A suppression needs nobody's agreement | A deviation needs a reason beside the code and an approval signed by a trusted key |
| Knows the language, not your board | Reads RAM, flash, FPU, heap policy, execution model and safety level from the hardware profile |
| Judges code only | Also judges build flags, boot configuration, CI scripts, test files and the analyser's own configuration |

A real analyser still sees far more than this pack does: types, data flow,
macro expansion, the full MISRA rule set. Keep it. The two work together: the
analyser finds deep defects in CI, and Aram makes sure nobody, human or AI,
can turn the analyser down, skip its tests or suppress its findings without a
signed approval.

## Set up in your firmware repository

You need Nool, a Rust toolchain with the WASM target
(`rustup target add wasm32-unknown-unknown`), and the Nool policy SDK that the
Nool installer places in `~/.nool/sdk`.

**1. Get the pack and point it at your SDK.**

```sh
git clone <this repository> embedded-policy-pack && cd embedded-policy-pack
```

Set the SDK path in `Cargo.toml` (`nool-aram-policy-sdk`) and in
`crates/emb-policy-glue/Cargo.toml` (`nool-policy-abi`) to your
`~/.nool/sdk/...` directories.

**2. Describe your board.** Edit `hardware-profile.toml`. Use the real part's
RAM and flash, the FPU your compiler flags enable, and your project's safety
level.

**3. Build and check.**

```sh
tools/check.sh        # unit tests, WASM build, and a real end-to-end run of every policy
```

**4. Install into your firmware repository.**

```sh
cd /path/to/your-firmware          # a Nool repository: run `nool init` if it is not one yet
for p in emb_security_baseline emb_firmware_integrity emb_c_subset; do
  nool admin plugin install /path/to/embedded-policy-pack/policy-src/$p/$p.toml --policy
done
```

Start with a few and add the rest one at a time: a policy judges each changed
file whole, so old violations in a file surface on its next edit.

**5. Allow agents and name reviewers.**

```sh
nool admin contract grant --role editor     # lets agents propose changes (Enterprise builds)
```

Add each reviewer's public key to `[attestation] checker_keys` in `nool.toml`
if you install `emb_safety_process`.

**6. Prove the gate is live.**

```sh
printf 'void f(void)\n{\n    goto out;\nout:\n    return;\n}\n' > src/smoke.c
nool propose --path src/smoke.c --intent "smoke" --why "check the gate" --solidify
# refused with [EMB-C-001] ... MISRA C:2012 Rule 15.1
rm src/smoke.c
```

From here, engineers and agents work as usual. [docs/GUIDE.md](docs/GUIDE.md)
covers daily use, deviations, approval, each policy in detail, and adoption on
an existing code base.

## A path for adoption

1. **Pilot (a day).** One repository, three policies: security baseline,
   firmware integrity, and the language subset you use. These rarely fire on
   healthy code and show the workflow.
2. **Fit the hardware (a week).** Set the profile, add `emb_hardware_fit`,
   `emb_resource_bounds` and `emb_runtime_safety`, and clear or deviate what
   they find in the files you touch.
3. **Close the loop.** Add `emb_safety_process`, name your reviewers, and move
   deviation approval from email into signed attestations.
4. **Make it yours.** Add your company's rules, your vendor's config keys and
   your requirement-tag format. See [CONTRIBUTING.md](CONTRIBUTING.md).

## What it is not

The checks read the text of each changed file. They are fast and they catch
what is visible there; they do not do type or data-flow analysis.

- It is not a certified static analyser and not a compliance claim. Keep your
  MISRA analyser, compiler warnings and tests in CI. The pack is the early
  gate in front of them.
- ISO 26262, IEC 62304, IEC 61508 and DO-178C are not cited clause by clause,
  because their text is not free. They act through the safety level in the
  profile. Every clause that is cited was checked against the document's text.
- Stack depth, timing, image size and behaviour on the device still need their
  own tools. [docs/GUIDE.md](docs/GUIDE.md) lists, per policy, what you must
  still verify.

## Repository map

| Path | Contents |
|---|---|
| `hardware-profile.toml` | The target and safety level every policy is built against |
| `crates/emb-rules/` | The rule engine: one module and one test set per policy, plain Rust, no Nool needed |
| `crates/emb-policy-glue/` | Connects the rules to the Nool policy SDK |
| `policy-src/` | One thin WASM policy crate per policy |
| `policies/` | The built policies installed in this repository |
| `tests/e2e/` | Real `nool propose` and `nool push` runs in throwaway repositories |
| `tools/` | `check.sh`, `build-policies.sh`, `new-policy.py`, `make-catalog.py` |
| `docs/GUIDE.md` | The engineer's guide |
| `standards/` | Catalog of 199 embedded standards with source, edition and availability |

## Contributing

New rules, vendor config keys and false-positive fixes are welcome.
[CONTRIBUTING.md](CONTRIBUTING.md) explains how to add a rule, what a change
must pass, and how to land it.
