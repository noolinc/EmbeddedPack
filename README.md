# Embedded Policy Pack

Nool Aram policies (Rust, compiled to WASM components) that refuse a change
which violates an embedded-systems standard. Each denial names the rule, the
file and line, and the clause it enforces:

    [EMB-C-001] src/smoke.c:3: goto (goto shall not be used) -- MISRA C:2012 Rule 15.1; BARR-C:2018 1.7c; Power of 10 Rule 1

## Policies

| Policy | Rules | Enforces |
|---|---|---|
| `emb_c_subset` | EMB-C-001..019 | C language subset: MISRA C:2012, BARR-C:2018, CERT C, Power of 10 |
| `emb_cpp_subset` | EMB-CPP-001..018 | C++ subset: AUTOSAR C++14, JSF AV C++ |
| `emb_resource_bounds` | EMB-RES-001..006 | Heap (per profile), recursion, alloca/VLA, loop bounds, function length, pointer depth |
| `emb_hardware_fit` | EMB-HW-001..006 | Fit to `hardware-profile.toml`: stack objects vs RAM, FPU, flash, exceptions, execution model |
| `emb_security_baseline` | EMB-SEC-001..011 | ETSI EN 303 645, CWE: secrets, weak crypto, unbounded copies, TLS verification, cleartext |
| `emb_runtime_safety` | EMB-RT-001..005 | BARR-C 6.5, NASA-GB-8719.13 4.6, CERT CON33-C: blocking calls in ISRs, interrupts left disabled, non-reentrant library calls, watchdog disabled or fed from an ISR |
| `emb_firmware_integrity` | EMB-FWI-001..007 | NIST SP 800-193, SP 800-218, UN R156, EN 303 645: silenced warnings, removed hardening, secure boot / rollback protection off, development signing keys, unauthenticated build downloads, open debug access |
| `emb_safety_process` | EMB-PRC-001..007 | MISRA Compliance:2020, NASA NPR 7150.2D: analysis suppressions, requirement traces, disabled tests, malformed deviations, weakened `.clang-tidy`, approval before push |

## Hardware profile

`hardware-profile.toml` holds RAM, flash, FPU, execution model (superloop,
RTOS, event-driven), heap policy and the safety level (ASIL, IEC 62304 class,
SIL). A policy cannot read the repository from its sandbox, so the profile is
compiled into every policy. After changing it:

    tools/build-policies.sh
    nool admin plugin install policy-src/<name>/<name>.toml --policy   # each policy

On a high-integrity profile (ASIL C/D, class C, SIL 3/4) the rules marked
strict (goto, heap, recursion, VLA, all security rules) cannot be deviated.

## How a change is judged

- The policy sees each changed file whole, so existing violations in a file
  block further edits to it until they are fixed or deviated.
- Checks are lexical (comments and strings are ignored). They catch what is
  visible in the text; they do not replace a MISRA-certified analyser.
- Test code (`tests/`, `test_*`, `*_test.*`) and adopted code (`third_party/`,
  `vendor/`, `external/`) are exempt.
- A Required rule is waived by a comment on or above the line, with a reason of
  at least 20 characters. Mandatory rules are never waived.

      /* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */

## Approval before push

`emb_safety_process` also runs at `nool push`. A knot is held as pending
evidence (exit 3) until a trusted key has attested it:

| The knot | Needs a trusted `pass` for |
|---|---|
| carries a deviation that waives a rule | `emb-deviation-review` |
| changes `hardware-profile.toml` | `emb-profile-review` |

    nool attest record --target <knot> --checker <reviewer> --assertion emb-deviation-review --verdict pass

A key is trusted when it is listed in `[attestation] checker_keys` in
`nool.toml`. An attestation signed by any other key, including the author's
own, does not release the push. On a profile with a safety level (ASIL A+,
IEC 62304 class B+, SIL 1+) every production source file that defines
functions needs a requirement tag in a comment, for example `@req SRS-123`.

## Standards

`standards/CATALOG.md` lists the 198 standards researched, with source,
edition and status. `standards/sources.json` records the URL and SHA-256 of
each fetched document. The documents are in `standards/specs/` and are not
tracked: most are copyrighted. A clause is cited only when it was checked
against the document's text or a free official document that corroborates it.

## Develop

    cargo test -p emb-rules            # one test per rule, named after the rule id
    tools/build-policies.sh <name>     # build to WASM
    tests/e2e/<name>.sh                # real `nool propose` in a throwaway repository

The policies depend on the Nool SDK at `~/.nool/sdk` (path set in `Cargo.toml`).
