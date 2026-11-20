# Embedded Policy Pack: engineer's guide

This guide is for embedded software engineers who work in a Nool-tracked
firmware repository where the pack is installed, and for the lead who sets it
up. It covers what the pack does for you, which policies to choose, how to set it up, how to work with
it day to day, what each of the eight policies checks, and what you still have
to verify yourself.

## 1. What the pack is

Eight Nool Aram policies (WASM components). Nool runs them on every change
before the change lands. A change that breaks a rule is refused, and the
refusal names the rule, the file and line, and the clause of the standard:

    [EMB-C-001] src/motor.c:42: goto (goto shall not be used) -- MISRA C:2012 Rule 15.1; BARR-C:2018 1.7c; Power of 10 Rule 1

The same gate applies to a person and to an AI agent. One policy also runs at
`nool push` and holds a change that needs an independent approval.

## 2. What you gain

| Today | With the pack |
|---|---|
| Rule violations show up in a CI analyser report after the code is written | The change is refused when it is proposed, with the clause, so it is fixed while the author (or agent) still has the context |
| An agent or a hurried engineer can get green by adding `NOLINT`, `-w`, skipping a test or editing `.clang-tidy` | Each of those is itself a refused change |
| Analysers do not know your board | Rules use your RAM, flash, FPU, heap policy and execution model from `hardware-profile.toml` |
| Deviations live in a spreadsheet and are approved by email | A deviation is a comment beside the code, and the change cannot be pushed until a trusted key has signed an approval for it |
| "Who allowed this?" needs archaeology | Every landed change and every approval is a signed record in the repository history |

What it is not: a certified static analyser, or a compliance claim. The checks
read the text of the changed files. Keep your MISRA analyser, compiler
warnings and tests in CI; the pack is the early, cheap gate in front of them.
Section 9 lists exactly what stays your job.

## 3. Choose your policies

Do not install all eight. Each policy you add is another set of rules every
change must pass, and on an existing code base each one surfaces old findings
in every file you touch. Pick the two or three that address the risks your
project actually has, run with them for a few weeks, and add more only when a
real problem calls for it.

### Start from your project

| Your project | Start with | Add later if needed | Usually skip |
|---|---|---|---|
| **Small bare-metal MCU in C** (superloop, kilobytes of RAM, no network) | `emb_hardware_fit`, `emb_resource_bounds`, `emb_c_subset` | `emb_runtime_safety` when interrupt code grows | `emb_cpp_subset`, `emb_security_baseline` |
| **RTOS firmware in C** (tasks, drivers, interrupts) | `emb_runtime_safety`, `emb_resource_bounds`, `emb_hardware_fit` | `emb_c_subset` | `emb_cpp_subset` |
| **Connected / IoT device** (Wi-Fi, BLE, cellular, OTA update) | `emb_security_baseline`, `emb_firmware_integrity`, `emb_runtime_safety` | `emb_hardware_fit` | Language subsets, unless a customer asks for MISRA |
| **Safety-related C** (automotive, medical, industrial; ASIL / class / SIL assigned) | `emb_c_subset`, `emb_resource_bounds`, `emb_safety_process` | `emb_runtime_safety`, `emb_hardware_fit` | `emb_cpp_subset` |
| **C++ application firmware** | `emb_cpp_subset`, `emb_resource_bounds`, `emb_hardware_fit` | `emb_runtime_safety` | `emb_c_subset` (but see the note on `.h` files) |
| **Bootloader or update component** | `emb_firmware_integrity`, `emb_security_baseline`, `emb_c_subset` | `emb_resource_bounds` | `emb_safety_process` unless the project has a safety level |
| **Large legacy code base** | `emb_firmware_integrity`, `emb_runtime_safety`, `emb_security_baseline` | One language or resource policy per quarter, after a baseline clean-up | `emb_c_subset` / `emb_cpp_subset` at first: they fire on most old files |
| **Team that relies heavily on AI agents** | Your project's row above, plus `emb_safety_process` | | |

These are starting points, not a classification. A motor controller with a
CAN bootloader is two rows; take what applies from each.

### Or start from the problem you have

| The problem you keep having | Policy |
|---|---|
| Code that works on the dev kit and overflows the stack or the flash on the real part | `emb_hardware_fit` |
| Heap fragmentation, recursion, or loops that never end in the field | `emb_resource_bounds` |
| Hard faults and hangs traced to interrupt handlers or a disabled watchdog | `emb_runtime_safety` |
| A customer or auditor asking for MISRA C or AUTOSAR C++ evidence | `emb_c_subset` or `emb_cpp_subset`, with `emb_safety_process` for deviations |
| Credentials, test keys or `http://` endpoints reaching a release | `emb_security_baseline` |
| A release built with secure boot off, warnings silenced, or the development key | `emb_firmware_integrity` |
| Suppressed warnings, skipped tests and waivers nobody approved | `emb_safety_process` |

### What each policy costs

| Policy | Install it when | Leave it out when | Needs from you | Findings on existing code |
|---|---|---|---|---|
| `emb_c_subset` | You follow, or must show, a C subset such as MISRA C or BARR-C | The code is C++, or mostly vendor HAL you will not rewrite | Nothing | Many: unbraced bodies, missing `default`, `<stdio.h>` |
| `emb_cpp_subset` | Your firmware is C++ and you follow AUTOSAR C++14 or a similar subset | The code is C | Nothing | Many: C-style casts, plain enums, `NULL` |
| `emb_resource_bounds` | No heap, or a strict memory and loop discipline | A Linux-class device with ample memory where heap use is normal (or set `heap = "allowed"`) | `heap`, `max_function_lines`; task and init functions named by convention | Some: long functions, `malloc`, `for (;;)` in oddly named functions |
| `emb_hardware_fit` | The part is small, has no or a single-precision FPU, or the execution model is strict | RAM and flash are generous and you use an FPU with double precision | An accurate profile; rebuild after each change to it | Depends on the part: on a small MCU, some; on a large one, few |
| `emb_security_baseline` | The device has any interface an attacker can reach, or holds keys | A sealed device with no communication and no secrets | Nothing | Some: `strcpy`, `sprintf`, `rand`; mandatory findings for any committed secret |
| `emb_runtime_safety` | You write interrupt handlers or use an RTOS | No interrupts of your own and no watchdog (rare) | ISRs named or attributed by convention; `execution_model` | Few |
| `emb_firmware_integrity` | You ship signed or updatable firmware, or want compiler warnings kept on | No build files in the repository | Nothing | Few |
| `emb_safety_process` | You have a deviation process, a safety level, or agents that might suppress findings | No reviewers available to sign approvals: pushes would wait forever | Reviewer keys in `checker_keys`; with a safety level, a requirement tag in every source file | Few without a safety level; with one, every untagged source file |

### Things to know before you choose

- **A policy is all or nothing.** There is no switch to turn one rule off.
  If a single rule does not fit your project, deviate it where it fires, or
  remove it from the policy's `RULES` list and rebuild (see
  `CONTRIBUTING.md`). If most of a policy does not fit, do not install it.
- **`.h` files are judged as C.** `emb_cpp_subset` checks `.hpp`, `.hh` and
  `.hxx` headers but not `.h`. In a C++ project with `.h` headers, either
  rename them or accept that those headers are outside the C++ rules; and do
  not install `emb_c_subset` there, or it will judge them as C.
- **The hardware profile matters only to some policies.** `emb_hardware_fit`,
  `emb_resource_bounds`, `emb_runtime_safety` and `emb_safety_process` read
  it. The safety level in it also decides, for every policy, whether strict
  rules can still be deviated. If you install only the language, security and
  integrity policies, the hardware keys can stay at their defaults.
- **`emb_safety_process` is the only one that acts at push.** Without it,
  deviations in the other policies work, but nobody has to approve them.
- **Edition limits.** Nool's Community edition loads up to 3 policies, Team
  up to 11. Three is a sensible number to start with in any case.
- **Removing a policy is allowed.** If one costs more than it gives,
  `nool admin plugin uninstall <name> --policy` takes it out. That is a recorded
  change, which is the point: the rule set is a decision, made in the open.

## 4. Set-up (once per repository)

Prerequisites: Nool 7.30 or later, a Rust toolchain, and the Nool policy SDK
in `~/.nool/sdk` (the workspace `Cargo.toml` and
`crates/emb-policy-glue/Cargo.toml` point at it by path; change the paths if
yours is elsewhere).

1. **Describe the target.** Edit `hardware-profile.toml` (section 5). This is
   the most important step: a wrong profile gives wrong refusals.
2. **Build.** `tools/build-policies.sh` runs the unit tests and builds every
   policy to WASM.
3. **Install.** For each policy:
   `nool admin plugin install policy-src/<name>/<name>.toml --policy`.
   Each install lands the policy and pins its hash in `nool.toml`.
4. **Let agents edit.** On an Enterprise build an agent needs a contract once
   any policy is installed: `nool admin contract grant --role editor`.
5. **Name the reviewers.** Add each reviewer's public key to
   `[attestation] checker_keys` in `nool.toml` and land it (a `nool.toml`
   change needs `--justification`). `nool attest record` prints the signer's
   key. Without a trusted key, changes that need approval can never be pushed.
6. **Check the installation** (section 8).

Install only the policies you chose in section 3.

### Adopting on an existing code base

A policy judges each changed file as a whole. A file with old violations is
refused on its next edit, whatever the edit was. Plan for it:

- Install one policy at a time, starting with the ones section 3 lists as
  having few findings on existing code.
- Before installing a policy, run its rules over the tree and fix or deviate
  what it finds. The rule engine is a plain Rust library
  (`emb_rules::check`), so a small script can do this without Nool.
- Move adopted code under `third_party/`, `vendor/` or `external/`; those
  paths are exempt.

## 5. The hardware profile

```toml
[hardware]
name = "generic-cortex-m4"
arch = "armv7e-m"
ram_bytes = 65536        # also 64k / 1m
flash_bytes = 262144
fpu = "single"           # none | single | double

[architecture]
execution_model = "rtos" # superloop | rtos | event-driven
heap = "none"            # none | init-only | allowed
max_stack_object_bytes = 0   # 0 = RAM/64, clamped to 64..4096
max_function_lines = 60

[safety]
iso26262_asil = "B"      # QM | A | B | C | D
iec62304_class = "B"     # none | A | B | C
iec61508_sil = 2         # 0..4
```

| Key | What it changes |
|---|---|
| `ram_bytes`, `max_stack_object_bytes` | Largest array or buffer allowed on the stack (EMB-HW-001) |
| `flash_bytes` | Under 128 KiB: `<iostream>`-family headers, float `printf` formats, exceptions and RTTI are refused (EMB-HW-003, -004) |
| `fpu` | `none`: `float` and `double` refused. `single`: `double` refused. `double`: no limit (EMB-HW-002) |
| `execution_model` | Which blocking and kernel calls are allowed where (EMB-HW-005), and whether non-reentrant library calls matter (EMB-RT-003) |
| `heap` | `none`: no allocation. `init-only`: allocation only in init functions, never freed. `allowed`: no limit (EMB-RES-001). Without a heap, exceptions are refused too |
| `max_function_lines` | Function length limit (EMB-RES-005) |
| `[safety]` | ASIL C/D, class C or SIL 3/4 make the *strict* rules non-deviatable. ASIL A+, class B+ or SIL 1+ turn on requirement tracing (EMB-PRC-002) |

The profile is compiled into the policies, because a policy cannot read the
repository from its sandbox. **After any edit: rebuild and reinstall every
policy.** Until then the old profile is still enforced. An unknown key or a
bad value is refused (EMB-HW-006), and a profile change needs an
`emb-profile-review` approval before push.

There is one profile per repository. A repository that builds for two boards
should use the more constrained board's values.

## 6. Daily work

### Reading a refusal

    [EMB-HW-001] src/log.c:18: 'buf' puts at least 2048 bytes on the stack; this target allows 1024 (RAM 65536 bytes) (a stack object shall fit the target's RAM budget) -- hardware profile: hardware.ram_bytes, architecture.max_stack_object_bytes | deviate: /* EMB-DEVIATION(EMB-HW-001): <reason> */ on or above the line

It gives the rule id, where, what was seen, the requirement, the clause, and
whether the rule can be deviated. Up to 12 findings are listed; the rest are
counted. Fix the code and propose again.

### Rule classes

| Class | Meaning |
|---|---|
| Required | Can be waived by a deviation |
| Required, strict | Can be waived, except on a high-integrity profile (ASIL C/D, class C, SIL 3/4) |
| Mandatory | Never waived. Fix the code |

### Deviating a rule

When the violation is intended and justified, put a comment on the line of
the finding or the line above it:

```c
/* EMB-DEVIATION(EMB-C-001): single cleanup exit for three resources, DR-7 */
goto cleanup;
```

- One comment waives one rule on one line. There is no file-wide or
  project-wide waiver.
- The reason must be at least 20 characters. Refer to your deviation record.
- In build and config files use that file's comment syntax
  (`# EMB-DEVIATION(...)`).
- A deviation that names an unknown rule, a rule that cannot be deviated, or
  gives a short reason is refused (EMB-PRC-004).

### Getting a deviation approved

With `emb_safety_process` installed, a change that carries a deviation lands
in the author's repository, but `nool push` holds it (exit code 3, "evidence
pending") until a reviewer whose key is trusted has attested
`emb-deviation-review` on it. A change to `hardware-profile.toml` needs
`emb-profile-review` in the same way.

The held change is not on the shared remote, so the reviewer cannot get it
from there. The reviewer takes it from the author instead, and sends back only
the signature:

```
 author's repository          reviewer's repository             shared remote
 ───────────────────          ─────────────────────             ─────────────
 1. change lands locally
 2. nool push  → held (exit 3)
        │
        └──── 3. nool pull <author's repository> ──▶ has the change
                 4. reads the code and the deviation
                 5. nool attest record … --push origin ─────▶ approval only
 6. nool attest wait <knot> --remote origin  ◀──────────────── approval
 7. nool push origin ────────────────────────────────────────▶ change
```

**Author**, after the push is held: tell the reviewer the knot id (it is in
the refusal message) and where your repository is.

**Reviewer**, in their own clone:

```sh
nool pull <author's repository>      # an SSH URL, a path on a shared machine, or a git bundle file
nool review <knot> --diff            # read the change and the deviation
nool attest record --target <knot> --checker <your name> \
    --assertion emb-deviation-review --verdict pass --push origin
```

`--push origin` sends the signed approval to the shared remote. It does not
send the change: the branch on the remote is untouched until the author
pushes.

**Author**, once the reviewer has signed:

```sh
nool attest wait <knot> --assertion emb-deviation-review --remote origin
nool push origin
```

Things to know:

- **Getting the change to the reviewer** is the one step that needs a route
  between the two people. `nool pull` reads any Git transport: SSH to the
  author's machine, a path on a shared build server, or a file made with
  `git bundle create review.bundle --all` and sent by any means.
- **A reviewer who rejects** records `--verdict fail` with a comment
  (`nool review <knot> --comment "…"`). The push stays held. The author fixes
  the code in a new change; the approval is needed on the change that carries
  the deviation.
- **An approval is for one exact change.** Edit the file again and the new
  change needs its own approval.
- **A key is trusted** when it is listed in `[attestation] checker_keys` in
  `nool.toml`. An attestation from any other key, including the author's own,
  does not release the push. Do not put an author's key in that list for
  changes they write, and never an agent's.
- **Small teams.** If only one person can review, pushes with deviations wait
  for that person. That is the intended cost; if it is too high for your
  project, do not install `emb_safety_process`.
- **A reviewer on the author's machine** (pairing, or a shared workstation)
  can sign there directly with their own key, without the pull.

Reviewer's questions: Is the rule really unavoidable here? Does the reason
match a deviation record? Is the risk assessed and are the precautions in the
code? Is the deviation as narrow as it can be?

### What is exempt

| Path | Treatment |
|---|---|
| `test/`, `tests/`, `unittest/`, `mocks/`, `testing/`, `test_*`, `*_test.*` | Exempt from all rules except disabled-test detection (EMB-PRC-003) |
| `third_party/`, `vendor/`, `external/`, `extern/`, `node_modules/` | Exempt from everything |
| Files over 2 MB, binary files | Not analysed |

### Working with an AI agent

Nothing extra is needed: the agent gets the same refusal text and fixes its
change. Two habits help. Tell the agent the profile (no heap, RTOS, ASIL B)
at the start of a task, so it writes fitting code first time. And review any
deviation an agent writes as carefully as its code: the push gate exists
because writing a plausible reason is easy.

## 7. The policies

Each section gives the rules, how the check decides, and what you must still
verify, because a text-level check cannot see it.

### 7.1 `emb_c_subset`: C language subset

Files: production `.c` and `.h`.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-C-001 | Required, strict | goto shall not be used | MISRA C:2012 Rule 15.1; BARR-C:2018 1.7c; Power of 10 Rule 1 |
| EMB-C-002 | Required, strict | setjmp/longjmp shall not be used | MISRA C:2012 Rule 21.4; Power of 10 Rule 1 |
| EMB-C-003 | Required | atof, atoi, atol and atoll shall not be used | MISRA C:2012 Rule 21.7 |
| EMB-C-004 | Required | abort, exit, _Exit, quick_exit and getenv shall not be used | MISRA C:2012 Rule 21.8 |
| EMB-C-005 | Required | the facilities of <signal.h> shall not be used | MISRA C:2012 Rule 21.5 |
| EMB-C-006 | Required | the features of <stdarg.h> shall not be used | MISRA C:2012 Rule 17.1 |
| EMB-C-007 | Required | <stdio.h> input/output shall not be used in production code | MISRA C:2012 Rule 21.6 |
| EMB-C-008 | Required | octal constants shall not be used | MISRA C:2012 Rule 7.1 |
| EMB-C-009 | Required | /* shall not appear inside a comment | MISRA C:2012 Rule 3.1; BARR-C:2018 2.1b |
| EMB-C-010 | Required | #define shall not be used on a reserved identifier | MISRA C:2012 Rule 21.1; SEI CERT C DCL37-C |
| EMB-C-011 | Required | every switch shall have a default | MISRA C:2012 Rule 16.4; BARR-C:2018 8.3b |
| EMB-C-012 | Required | an unconditional break shall end every switch clause | MISRA C:2012 Rule 16.3 |
| EMB-C-013 | Required | the body of if/else/for/while/do shall be a braced block | MISRA C:2012 Rule 15.6; BARR-C:2018 1.3a |
| EMB-C-014 | Required | an assignment shall not be used as a condition | MISRA C:2012 Rule 13.4; BARR-C:2018 8.2c; SEI CERT C EXP45-C |
| EMB-C-015 | Required | floating-point values shall not be tested for equality | BARR-C:2018 5.4b.iv |
| EMB-C-016 | Required | a loop counter shall not have floating type | MISRA C:2012 Rule 14.1; SEI CERT C FLP30-C |
| EMB-C-017 | Required | ## and variadic macros shall not be used | MISRA C:2012 Rule 20.10; Power of 10 Rule 8 |
| EMB-C-018 | Required | the register and auto keywords shall not be used | BARR-C:2018 1.7a, 1.7b |
| EMB-C-019 | Required | bsearch and qsort shall not be used | MISRA C:2012 Rule 21.9 |

How it decides: by tokens, after comments and strings are removed. It does
not expand macros or follow includes.

Verify yourself:
- A banned construct hidden inside a macro or behind a function pointer is
  not seen. Your analyser must cover that.
- This is 19 of the MISRA C:2012 guidelines, the ones decidable from text.
  Essential-type rules, pointer arithmetic, initialisation, side effects and
  undefined behaviour need a real analyser.
- EMB-C-007 exists to keep `printf` out of production. If you use a
  lightweight logger, name it something else; do not deviate `<stdio.h>`
  everywhere.

### 7.2 `emb_cpp_subset`: C++ language subset

Files: production C++ sources and headers (`.cpp`, `.cc`, `.cxx`, `.hpp`,
`.hh`, `.ino`, …).

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-CPP-001 | Required | traditional C-style casts shall not be used | AUTOSAR C++14 A5-2-2; JSF AV Rule 185 |
| EMB-CPP-002 | Required | reinterpret_cast shall not be used | AUTOSAR C++14 A5-2-4 |
| EMB-CPP-003 | Required | a cast shall not remove const or volatile | AUTOSAR C++14 A5-2-3 |
| EMB-CPP-004 | Required | only nullptr shall be used as the null pointer constant | AUTOSAR C++14 A4-10-1 |
| EMB-CPP-005 | Required, strict | goto shall not be used | AUTOSAR C++14 A6-6-1; JSF AV Rule 189 |
| EMB-CPP-006 | Required | <cstdio>, <csetjmp>, <csignal>, <ctime> and <clocale> shall not be used | AUTOSAR C++14 M27-0-1, M17-0-5, M18-7-1, M18-0-4, A18-0-3; JSF AV Rules 19-22, 25 |
| EMB-CPP-007 | Required | atoi/atof/atol, abort/exit/getenv/system, rand and setjmp/longjmp shall not be used | AUTOSAR C++14 A18-0-2, M18-0-3, A26-5-1, M17-0-5; JSF AV Rules 20, 23, 24 |
| EMB-CPP-008 | Required | functions shall not be defined with the ellipsis notation | AUTOSAR C++14 A8-4-1; JSF AV Rule 108 |
| EMB-CPP-009 | Required | auto_ptr, vector<bool>, std::bind, register, long double, asm and union shall not be used | AUTOSAR C++14 A18-1-3, A18-1-2, A18-9-1, A7-1-4, A0-4-2, A7-4-1, A9-5-1; JSF AV Rule 153 |
| EMB-CPP-010 | Required | using-directives shall not be used in header files | AUTOSAR C++14 M7-3-6 |
| EMB-CPP-011 | Required | enumerations shall be scoped enum classes | AUTOSAR C++14 A7-2-3 |
| EMB-CPP-012 | Required | every switch shall end in a default clause | AUTOSAR C++14 M6-4-6; JSF AV Rule 194 |
| EMB-CPP-013 | Required | a break or throw shall end every non-empty switch clause | AUTOSAR C++14 M6-4-5; JSF AV Rule 193 |
| EMB-CPP-014 | Required | the body of if/else/for/while/do shall be a compound statement | AUTOSAR C++14 M6-3-1, M6-4-1; JSF AV Rule 59 |
| EMB-CPP-015 | Required | octal constants shall not be used | AUTOSAR C++14 M2-13-2; JSF AV Rule 149 |
| EMB-CPP-016 | Required | assignment operators shall not be used in sub-expressions | AUTOSAR C++14 M6-2-1 |
| EMB-CPP-017 | Required | floating-point expressions shall not be tested for equality | AUTOSAR C++14 M6-2-2; JSF AV Rule 202 |
| EMB-CPP-018 | Required | a loop counter shall not have floating type | AUTOSAR C++14 A6-5-2; JSF AV Rule 197 |

How it decides: by tokens. A C-style cast is recognised from a parenthesised
type name; `(std::max)(a, b)` is not treated as a cast.

Verify yourself:
- Clauses are from AUTOSAR C++14 and JSF AV C++. MISRA C++:2023 is not cited,
  because its text is not free. If your project claims MISRA C++:2023, map
  these rules to it in your compliance matrix.
- Casts inside templates and macros, and conversions done by constructors,
  are not seen.
- Exception policy is in `emb_hardware_fit` (EMB-HW-004), not here.

### 7.3 `emb_resource_bounds`: memory, stack and loops

Files: production C and C++.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-RES-001 | Required, strict | dynamic memory shall not be used beyond what the hardware profile allows | MISRA C:2012 Dir 4.12, Rule 21.3; Power of 10 Rule 3; JSF AV Rule 206; AUTOSAR C++14 A18-5-1, A18-5-2 |
| EMB-RES-002 | Required, strict | functions shall not call themselves | MISRA C:2012 Rule 17.2; Power of 10 Rule 1; JSF AV Rule 119; AUTOSAR C++14 A7-5-2 |
| EMB-RES-003 | Required, strict | alloca and variable-length arrays shall not be used | MISRA C:2012 Rule 18.8; SEI CERT C ARR32-C |
| EMB-RES-004 | Required | every loop shall have a fixed upper bound, except a task's main loop | Power of 10 Rule 2 |
| EMB-RES-005 | Required | a function shall fit on one page | Power of 10 Rule 4 |
| EMB-RES-006 | Required | no more than two levels of pointer indirection | MISRA C:2012 Rule 18.5; JSF AV Rule 170 |

How it decides:
- Heap: calls to `malloc`/`calloc`/`realloc`/`free`, `new`/`delete`, and the
  RTOS allocators, judged against `architecture.heap`. With `init-only`, a
  function counts as init when its name is `main` or contains `init`,
  `setup`, `boot` or `startup`.
- Recursion: a function that calls itself by name.
- Unbounded loops: `while (1)`, `for (;;)` and similar are allowed only in
  functions whose name shows they run forever (`main`, or containing `task`,
  `thread`, `loop`, `idle`, `worker`, `run`, `handler`, `fault`, `panic`, …).
- Function length: body lines against `max_function_lines`.

Verify yourself:
- **Indirect recursion** (A calls B calls A) and recursion through function
  pointers are not detected. Check the call graph with your analyser.
- **Total stack depth** is not computed. Run a stack-usage analysis
  (`-fstack-usage`, a call-graph tool, or measurement with a watermark).
- A loop with a condition is accepted as bounded; whether the bound is
  actually reached is yours to show.
- Name your task and init functions by the conventions above, or the heap and
  loop rules will misjudge them.

### 7.4 `emb_hardware_fit`: fit to the target

Files: production C and C++, plus `hardware-profile.toml` itself.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-HW-001 | Required | a stack object shall fit the target's RAM budget | hardware profile: hardware.ram_bytes, architecture.max_stack_object_bytes |
| EMB-HW-002 | Required | floating point shall match the target's FPU | hardware profile: hardware.fpu; BARR-C:2018 5.4a |
| EMB-HW-003 | Required | stream and float-formatting libraries shall not be used on small flash | hardware profile: hardware.flash_bytes |
| EMB-HW-004 | Required | exceptions and RTTI shall not be used without a heap or on small flash | hardware profile: architecture.heap, hardware.flash_bytes; JSF AV Rule 208 |
| EMB-HW-005 | Required | code shall follow the declared execution model | hardware profile: architecture.execution_model |
| EMB-HW-006 | Mandatory | hardware-profile.toml shall parse | hardware profile |

How it decides:
- Stack objects: a local array whose size is a literal (times the element
  size) is compared with the limit. `static` and global objects are not stack
  objects.
- FPU: the keywords `float` and `double`. At most three findings per file.
- Execution model (EMB-HW-005):

| Model | Refused |
|---|---|
| `superloop` | Any kernel call (`xTaskCreate`, `vTaskDelay`, `k_sleep`, `osDelay`, …); busy delays (`HAL_Delay`, `delay`, `_delay_ms`, …) outside init functions |
| `rtos` | Busy delays outside init functions (block in the kernel instead) |
| `event-driven` | In an event handler: any delay or kernel sleep, a wait-forever timeout (`portMAX_DELAY`, `K_FOREVER`, …), an endless loop |

An event handler is a function named `on_*`, `handle_*`, `On*`, `Handle*`,
`*_handler`, `*_cb`, `*_callback` or `*Callback`.

Verify yourself:
- The profile matches the real part: RAM and flash of the actual variant, the
  FPU actually enabled by your compiler flags (`-mfpu`, `-mfloat-abi`).
- **The image fits.** The pack does not read the map file. Check flash and
  RAM use from the linker output in CI.
- Arrays sized by a macro or `sizeof` expression are not measured; large
  structs passed or returned by value are not measured.
- Float use through `typedef`s (`float32_t`) or library calls (`sinf`) is not
  seen by EMB-HW-002.
- Timing: nothing here proves a deadline is met.

### 7.5 `emb_security_baseline`: device security

Files: production C and C++; `KEY=value` lines in build, config and script
files.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-SEC-001 | Mandatory | credentials and keys shall not be hard-coded | ETSI EN 303 645 Provision 5.1-1, 5.4-3; CWE-798 |
| EMB-SEC-002 | Mandatory | private key material shall not be stored in the repository | ETSI EN 303 645 Provision 5.4-1, 5.4-3; CWE-798 |
| EMB-SEC-003 | Required, strict | broken or weak cryptographic algorithms shall not be used | ETSI EN 303 645 Provision 5.5-1; CWE-328 |
| EMB-SEC-004 | Required, strict | rand() shall not be used | CWE-338; SEI CERT C MSC30-C, MSC32-C; AUTOSAR C++14 A26-5-1 |
| EMB-SEC-005 | Mandatory | gets() shall not be used | CWE-242 |
| EMB-SEC-006 | Required, strict | unbounded string functions shall not be used | CWE-120, CWE-676; SEI CERT C STR31-C |
| EMB-SEC-007 | Mandatory | certificate verification shall not be disabled | ETSI EN 303 645 Provision 5.5-1; CWE-295 |
| EMB-SEC-008 | Required, strict | endpoints shall not use cleartext protocols | ETSI EN 303 645 Provision 5.5-1, 5.5-7; CWE-319 |
| EMB-SEC-009 | Required, strict | a format string shall be a literal | CWE-134; SEI CERT C FIO30-C |
| EMB-SEC-010 | Required, strict | secrets shall be compared in constant time | CWE-208 |
| EMB-SEC-011 | Required, strict | commands shall not be run through a shell or exec | CWE-78; SEI CERT C ENV33-C; MISRA C:2012 Rule 21.21 (Amendment 2) |

How it decides:
- Credentials: a string literal assigned to a name that contains `password`,
  `passwd`, `secret`, `token`, `api_key`, `private_key` and similar; a PEM
  private-key block anywhere.
- Weak crypto: use of MD5, SHA-1, DES, RC4 and similar by name.
- TLS: known switches that turn certificate verification off
  (`setInsecure()`, verify mode none, skip-common-name options).
- Cleartext: `http://`, `mqtt://`, `ftp://`, `telnet://` endpoints in strings
  (local addresses are allowed).

Verify yourself:
- A secret that does not look like one (an unlabelled byte array, a key split
  across files) is not found. Run a secret scanner too, and keep keys in a
  secure element or provisioning step.
- A key that was ever landed is in the history. Refusal protects the future;
  rotate anything that was committed before.
- Crypto used correctly: modes, IVs, key lengths, certificate pinning and
  entropy sources are not checked.
- EN 303 645 has many provisions outside code (vulnerability disclosure,
  update policy, user data deletion). The pack covers the ones visible in
  source.

### 7.6 `emb_runtime_safety`: interrupts, critical sections, watchdog

Files: production C and C++; watchdog keys in Kconfig-style files.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-RT-001 | Required | an ISR shall not block, allocate, print or call the kernel's task-level API | BARR-C:2018 6.5 |
| EMB-RT-002 | Required | a function that disables interrupts shall restore them | BARR-C:2018 6.5 |
| EMB-RT-003 | Required | non-reentrant library functions shall not be used with concurrent tasks | SEI CERT C CON33-C; hardware profile: architecture.execution_model |
| EMB-RT-004 | Required, strict | the watchdog shall not be disabled | NASA-GB-8719.13 4.6 |
| EMB-RT-005 | Required, strict | the watchdog shall not be fed from an interrupt | NASA-GB-8719.13 4.6 |

How it decides:
- A function is an ISR when its name ends in `_IRQHandler`, `_Handler`
  (capitalised), `_ISR`, `_isr`, `_irq`, starts with `ISR_` or `isr_`, is the
  AVR `ISR(...)` form, or carries an interrupt attribute (`__interrupt`,
  `IRAM_ATTR`, …).
- In an ISR: allocation, `printf`-family, delays, mutex and semaphore takes,
  and FreeRTOS `xQueue…`/`xSemaphore…`/`xTaskNotify…`/`xTimer…` calls without
  `FromISR` are refused.
- Critical sections: a function that calls a disable (`__disable_irq`, `cli`,
  `taskENTER_CRITICAL`, `irq_lock`, …) must contain a matching enable. A
  function whose name says it is the disabling half (`…enter…`, `…lock…`,
  `…disable…`, `…critical…`) or a fault path (`panic`, `fault`, `halt`,
  `reset`) is exempt.
- Watchdog: disable calls (`wdt_disable`, `IWDG_Disable`, …) and config keys
  (`CONFIG_WATCHDOG=n`, `CONFIG_WDT_DISABLE_AT_BOOT=y`, …); feed calls inside
  an ISR.

Verify yourself:
- **Name ISRs by convention**, or register them with an attribute. A handler
  installed through a vector table under another name is not recognised.
- Only direct calls inside the ISR body are seen. A helper that the ISR calls
  and that blocks is not found.
- Enable-on-every-path is not proved: an early `return` between disable and
  enable passes the check. Review critical sections by hand.
- **Shared data**: missing `volatile`, races between ISR and task, and
  priority inversion are not detected.
- The watchdog is fed from the right place: after a real health check of all
  tasks, with a timeout that matches your fault-reaction time.

### 7.7 `emb_firmware_integrity`: build and boot chain

Files: production build files (Makefile, CMake, `*.mk`, `platformio.ini`, …),
config files (`sdkconfig*`, `prj.conf`, `*.conf`, YAML), scripts, and two
checks in C/C++.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-FWI-001 | Required, strict | compiler diagnostics shall not be silenced or demoted | Power of 10 Rule 10; CWE-1127; NIST SP 800-218 PW.6.2 |
| EMB-FWI-002 | Required, strict | exploit mitigations shall not be turned off | NIST SP 800-218 PW.6.1, PW.6.2 |
| EMB-FWI-003 | Mandatory | secure boot, signature verification and flash encryption shall stay enabled | ETSI EN 303 645 Provision 5.7-1, 5.3-10; NIST SP 800-193 4.2.1.3; UN Regulation No. 156 7.2.1.1; CWE-494 |
| EMB-FWI-004 | Required, strict | rollback protection shall stay enabled | NIST SP 800-193 4.2.1.3 |
| EMB-FWI-005 | Mandatory | published development keys shall not sign firmware | ETSI EN 303 645 Provision 5.4-3; CWE-798 |
| EMB-FWI-006 | Required, strict | the build shall not run or fetch unauthenticated downloads | CWE-494, CWE-829; NIST SP 800-218 PW.4.1 |
| EMB-FWI-007 | Required, strict | debug and readout access shall be closed | NIST SP 800-193 4.2.1.3; ETSI EN 303 645 Provision 5.6-3; CWE-489 |

How it decides:
- Flags: `-w`, `-Wno-error…`, `-fpermissive`, and `-Wno-` of a
  safety-relevant warning (`return-type`, `uninitialized`, `format`,
  `array-bounds`, `implicit-function-declaration`, …). Style warnings such as
  `-Wno-unused-parameter` are allowed.
- Hardening: `-fno-stack-protector`, `_FORTIFY_SOURCE=0`, `execstack`,
  `norelro`.
- Boot: ESP-IDF and MCUboot/Zephyr keys for secure boot, signature type,
  flash encryption, anti-rollback and JTAG, both `KEY=n` and
  `# KEY is not set`.
- Keys: the file names of MCUboot's published keys (`root-rsa-2048.pem`, …).
- Fetch: `curl`/`wget` piped into a shell or interpreter; a download or
  dependency URL over `http://`.

Verify yourself:
- **Absence is not detected.** The pack refuses turning protection off; it
  does not check that secure boot, signing or `-Wall -Werror` were ever
  turned on. Confirm the final config (`sdkconfig`, `.config`) and compiler
  command line of a release build.
- Vendors not listed (STM32 option bytes beyond RDP level 0, NXP HAB, TI,
  Nordic APPROTECT) need their own check; add keys to
  `crates/emb-rules/src/integrity.rs`.
- The device: fuses burned, debug port locked, production key in the HSM,
  rollback counter working. Test on hardware.
- Downloads over https are accepted without a checksum. Pin hashes.

### 7.8 `emb_safety_process`: process evidence

Files: all production and test files except third-party; `.clang-tidy`.

| Rule | Class | Requirement | Clauses |
|---|---|---|---|
| EMB-PRC-001 | Required | analysis findings shall not be suppressed without a deviation | MISRA Compliance:2020 4.1; Power of 10 Rule 10; CWE-1127; NASA NPR 7150.2D [SWE-185] |
| EMB-PRC-002 | Required | safety-classified source shall trace to a requirement | NASA NPR 7150.2D 3.12.1 [SWE-052]; hardware profile: safety |
| EMB-PRC-003 | Required | tests shall not be disabled or skipped | NASA NPR 7150.2D 4.5.3 [SWE-066] |
| EMB-PRC-004 | Mandatory | a deviation shall name a deviatable rule and give its reason | MISRA Compliance:2020 4.2 |
| EMB-PRC-005 | Mandatory | a deviation shall be authorized by an independent reviewer before push | MISRA Compliance:2020 4.1 |
| EMB-PRC-006 | Required | the static-analysis configuration shall not be weakened | Power of 10 Rule 10; MISRA Compliance:2020 4.1 |
| EMB-PRC-007 | Mandatory | a hardware-profile change shall be reviewed before push | hardware profile |

How it decides:
- Suppressions: `NOLINT`, `cppcheck-suppress`, `coverity[…]`, `PRQA S`,
  `lint -e`, `NOSONAR`, `parasoft-suppress`, `polyspace`, `codeql[…]` in a
  comment; `#pragma … diagnostic ignored`, `#pragma warning(disable…)`,
  `#pragma GCC system_header`.
- Trace: with a safety level in the profile, a production C/C++ file that
  defines at least one function needs a tag in a comment: `@req`, `@trace`,
  `@satisfies`, `REQ-`, `SRS-`, `SWR-`, `SYS-`, `LLR-`, `HLR-` or
  `Implements:`.
- Disabled tests: `DISABLED_` test names, `TEST_IGNORE`, `IGNORE_TEST`,
  `GTEST_SKIP`, `ztest_test_skip`, `@pytest.mark.skip`/`xfail`,
  `@unittest.skip`, and `#if 0` in a test file.
- `.clang-tidy`: `-*` with nothing enabled, a whole `cert`, `bugprone`,
  `clang-analyzer` or `misra` group disabled, or empty `WarningsAsErrors`.
- At push: see "Getting a deviation approved".

Verify yourself:
- **The tag is only checked to exist.** That the requirement id is real,
  that it describes this code, and that a test traces back to it is your
  traceability tool's job.
- Tests exist, run and pass, and coverage meets your target. The pack only
  stops tests being switched off.
- A deleted test is not detected. Review test removals.
- The approval is as good as the reviewer. Keep `checker_keys` short, and
  never add an agent's key or the author's.
- The deviation record itself (risk assessment, precautions) lives in your
  quality system; the comment only points to it.

## 8. Checking the installation

Run these after set-up, after every profile change, and after every Nool
upgrade.

| Check | How | Expected |
|---|---|---|
| Rules behave | `cargo test -p emb-rules` | All tests pass (one test per rule, named after the rule id) |
| Policies enforce through Nool | `tests/e2e/<policy>.sh` for each policy | `PASS`. Each script proposes bad and good files in a throwaway repository |
| The right set is installed | `nool admin plugin list` | The policies you chose, no warnings you have not understood |
| The profile in force is yours | Propose a file with a rule violation | The last line of the refusal, `profile: …`, shows your values |
| The push gate works | Push a change with a deviation before approving it | Exit code 3, "evidence pending", naming EMB-PRC-005 |
| Reviewers are trusted | `nool attest show <knot>` after a reviewer attests | The approval is shown as trusted |

A smoke test you can run in any installed repository:

```sh
printf 'void f(void)\n{\n    goto out;\nout:\n    return;\n}\n' > src/smoke.c
nool propose --path src/smoke.c --intent "smoke" --why "check the gate" --solidify   # must be refused with EMB-C-001
rm src/smoke.c
```

## 9. What stays your responsibility

| Area | Why the pack cannot do it | Use |
|---|---|---|
| Full MISRA / AUTOSAR / CERT coverage | Needs types, data flow and macro expansion | A qualified static analyser in CI |
| ISO 26262, IEC 62304, IEC 61508, DO-178C clauses | Their text is not free, so no clause is cited; they act only through the safety level in the profile. Free overviews (such as ROHM's ISO 26262 white paper, listed in the catalog) explain the standard but carry no clause text to check a rule against | Your licensed copy, safety plan and compliance matrix |
| Stack depth, timing, image size | Not visible in the changed file | Stack analysis, WCET or measurement, map-file checks |
| Concurrency correctness | Needs whole-program analysis | Review, race detectors, RTOS-aware analysis |
| Behaviour on the device | Needs hardware | Hardware-in-the-loop and security testing |
| Existing code | Only changed files are judged | A one-time baseline run before adoption |
| Tool qualification | The pack is not qualified under any standard | Treat it as an aid, not as evidence in a safety case |

## 10. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| A one-line edit is refused for something elsewhere in the file | Files are judged whole. Fix or deviate the old findings |
| A refusal mentions values you changed in the profile | Policies were not rebuilt and reinstalled after the edit |
| "Reification failed" on a C file | Nool also compiles proposed C/C++ files; that is the compiler, not the pack |
| An agent's edit is refused as unauthorised | No editor contract: `nool admin contract grant --role editor` |
| `nool push` exits 3 forever | No trusted reviewer key, the approval was signed by an untrusted key, or it has not been fetched yet (`nool attest wait <knot> --remote origin`). Check `checker_keys` and `nool attest show <knot>` |
| The reviewer cannot find the change on the remote | A held change is never on the remote. The reviewer pulls it from the author's repository (section 6, "Getting a deviation approved") |
| A task function is refused for `for (;;)` or an init function for `malloc` | Its name does not follow the conventions in 7.3 |
| An ISR that blocks is not refused | Its name or attribute is not recognised (7.6) |
| A false positive | Deviate it with the reason, and report it; the rule's test file is the place to add the case |

## 11. Reference

- Rule source and tests: `crates/emb-rules/src/` (one module per policy).
- All rules as tables: `cargo run -p emb-rules --example rule_table`.
- Standards researched, with source and edition: `standards/CATALOG.md`.
- A clause is cited only when it was checked against the document's text or
  a free official document that corroborates it.
