# Contributing

This pack is only useful if engineers trust it: a rule must fire when it
should, stay quiet when it should not, and cite a clause that really says what
the rule claims. The process below exists to keep that true.

## Before you start

You need Nool 8.0.2 or later, Rust with `wasm32-unknown-unknown`, and the
Nool policy SDK (`~/.nool/sdk`). If you haven't used Nool before, read
[Nool basics](docs/GUIDE.md#nool-basics) first. This repository is itself
tracked by Nool, so you land changes with `nool propose`, not `git commit`.

Both `Cargo.toml` and `crates/emb-policy-glue/Cargo.toml` refer to the SDK by
absolute path. Point them at yours with the `sed` command in
[step 1 of the README](README.md#1-install-the-tools-and-clone-the-pack),
then run:

```sh
tools/check.sh
```

It must end with `ALL CHECKS PASS` before you change anything. If it does
not, fix your set-up first.

### Getting a Nool licence to contribute

This repository has all eight policies installed, which is more than the
Community edition loads, so working on it needs the Enterprise edition. If you
want to contribute and do not have it, ask for a contributor licence by email
at **help@nool.dev**. Include:

- **Your background**: your embedded or tooling experience, and the standards
  you work with.
- **Your purpose**: what you want to add or fix (a rule, a vendor's config
  keys, a false positive, a new policy).
- **Your previous open-source contributions**: links to repositories, pull
  requests or patches you have made elsewhere.

This repository is tracked by Nool. Do not use `git commit` or `git push`;
they are blocked. Changes land with `nool propose` (see "Landing").

## How the pack is built

```
hardware-profile.toml ──compiled into──┐
crates/emb-rules/src/<module>.rs       │  rules + unit tests (plain Rust)
crates/emb-policy-glue/                ├─ changed files -> findings -> denial
policy-src/<policy>/                   │  thin WASM crate, one per policy
tests/e2e/<policy>.sh                  ┘  real nool runs in a throwaway repo
```

A policy receives each changed file as its full new content. It cannot read
the repository, which is why the hardware profile is compiled in.

## The order of work

Follow this order for every rule. Reviewers will ask for each step.

1. **Find the clause and read it.** Cite a clause only if you have checked it
   against the document's text, or against a free official document that
   corroborates it. A clause number from memory, a blog or a vendor overview
   is not enough. If the standard is paywalled and you have no licensed copy,
   do not cite it. Record new sources in `standards/` with
   `tools/make-catalog.py`.
2. **Write the acceptance criteria.** In one sentence each: what is refused,
   what must not be refused, who is exempt, whether it can be deviated.
3. **Write the test first**, named after the rule id (see below).
4. **Implement the rule.**
5. **Add an end-to-end case.**
6. **Update the guide**, run `tools/check.sh`, and land.

One rule, or one tightly related group, per change.

## Adding a rule to an existing policy

Each policy's rules are declared at the top of its module, for example in
`crates/emb-rules/src/runtime.rs`:

```rust
rules! {
    ISR_BLOCKING = "EMB-RT-001", false, "an ISR shall not block, allocate, print or call the kernel's task-level API",
        "BARR-C:2018 6.5";
}
```

- **Id**: the next free number in the pack's series. Never reuse or renumber
  an id; deviations in users' code refer to it.
- **Title**: one requirement, written as "X shall (not) …".
- **Class**: `Mandatory` (never deviated; use only when no legitimate
  exception exists, such as a private key in the repository), `Required`, or
  `Required` with `strict` (not deviatable on a high-integrity profile).
- **Refs**: the verified clauses, most authoritative first.

Report a finding with `s.hit(&RULE, line, "what was seen")`. The detail should
name the identifier and say why, because it is what the engineer reads.

### The test

```rust
#[test]
fn rt001_blocking_call_in_isr() {
    rule(
        "EMB-RT-001",
        "BARR-C:2018 6.5",                                     // the clause must appear in the denial
        "void USART1_IRQHandler(void) { printf(\"rx\\n\"); }",  // must fire
        "void USART1_IRQHandler(void) { rx_flag = 1U; }",      // must not fire
    );
}
```

Every rule needs:

- a test named `<pack><number>_<what>` that shows it firing and the clause in
  the denial;
- at least one **near miss** that must not fire (the correct way to do the
  same thing). Most bugs in this pack have been false positives, and one false
  positive costs more trust than ten missed findings;
- coverage in the pack's `*_does_not_flag_*` and `*_scope` tests when the rule
  adds a new kind of construct;
- a line in the pack's `malformed_*_never_panics` test if it parses anything
  new. A policy that panics refuses every change.

### Engine constraints

The rule code runs inside a WASM sandbox with a fixed fuel budget and strict
float checks, so:

- no file or network access, no clocks, no environment;
- no floating point;
- no recursion (walk with an explicit stack);
- no indexing that can go out of bounds on truncated input (`get`, not `[]`,
  past the current token);
- work from tokens (`cx.toks()`), not raw text, for C and C++, so that
  comments and strings are ignored;
- respect scope: `cx.is_production()` unless the rule is about tests, and
  never judge third-party paths.

### Hardware-dependent rules

If the right answer depends on the target, read it from `cx.profile` and test
at least two profiles. Add a profile key only when no existing one fits: add
it to `Profile::parse` in `profile.rs`, to `hardware-profile.toml` with a
comment, and to section 5 of the guide.

## Adding a vendor's config keys or API names

Most extensions are a new entry in a list (`WDT_OFF`, `BOOT_MUST_NOT_BE_N`,
`IRQ_OFF`, `BLOCKING`, …). Add the name, add one firing case and one near miss
to the existing rule's test, and say in the change which vendor document the
name comes from.

## Adding a policy

1. Add a variant to `Pack` in `crates/emb-rules/src/model.rs`.
2. Write `crates/emb-rules/src/<module>.rs` with `pub static RULES` and
   `pub fn run(s: &mut Sink)`. Copy the shape of `runtime.rs`.
3. Scaffold and register it:
   `tools/new-policy.py <module> <Pack> emb_<name> "<description>"`.
4. Write `tests/e2e/emb_<name>.sh`. Use the helpers in `tests/e2e/lib.sh`
   (`setup_repo`, `expect_refused`, `expect_landed`, `finish`). Nool compiles
   proposed C and C++ files, so fixtures must be self-contained, compilable
   code: declare what you call.
5. Add a section 7.x to `docs/GUIDE.md` with the rule table, "How it decides"
   and "Verify yourself", a row in each table of section 3 (when to install it,
   what it costs), and a row to the README table.

## Updating the guide

`docs/GUIDE.md` carries a table of every rule. A test fails when a rule's id,
class, title or clauses differ from the guide, and prints the rows to paste:

```sh
cargo run -p emb-rules --example rule_table     # all tables, generated from the code
```

When a rule's behaviour changes, also update its policy's "How it decides" and
"Verify yourself" lists by hand. Be honest about what a text-level check
cannot see; that list is why engineers can rely on the rest.

## What a change must pass

| Check | Command | Passes when |
|---|---|---|
| Unit tests, one per rule | `cargo test -p emb-rules` | All pass |
| Guide matches the rules | (same command) | `guide_lists_every_rule_as_built` passes |
| No warnings | `cargo build` | No compiler warnings |
| Policies build to WASM | `tools/build-policies.sh` | Every policy builds |
| End to end through Nool | `tests/e2e/<policy>.sh` | `PASS` for every policy your change touches; all of them if you changed the lexer, model, engine or glue |
| All of the above | `tools/check.sh` | `ALL CHECKS PASS` |

`tools/check.sh <policy>` runs the unit tests and one policy's build and
end-to-end test while you iterate. Run the full script before you land.

## Landing

```sh
nool announce intent --intent "<what>" --target-nodes <paths>
nool propose --path <file> [--path <file>…] \
    --intent "Add EMB-RT-006: <requirement>" \
    --why "<the defect it prevents, and the clause>" --solidify
nool push origin
```

`announce` tells other contributors and agents which files you are editing,
so two people don't change the same rule at once. `propose --solidify` runs
the build, the tests and the installed policies, and lands the change only if
all of them pass. A refusal saves nothing, so fix the cause (see the table
below) and run the same command again. `push` sends your landed change to
the shared repository.

Things that refuse a landing here, and what to do:

| Refusal | Cause | Fix |
|---|---|---|
| `POLICY_DENY … EMB-…` | The pack is installed on its own repository and your change broke one of its rules (usually a test fixture outside `tests/`) | Fix the file, or move the fixture under `tests/` |
| `Reification failed` | Nool builds and tests the change before it lands | Make `cargo test` pass locally |
| `BLAST_RADIUS` | The change touches widely used files (lexer, model, engine) | Add `--justification "<why the wide change is needed>"` |
| `CONFIG_CHANGE` | The change touches `nool.toml` | Add `--justification` written for that change |
| Unauthorised agent edit | An agent has no contract | The owner runs `nool admin contract grant --role editor` |
| `evidence pending` at push | The change edits `hardware-profile.toml` or carries a deviation | A trusted reviewer records the `emb-profile-review` or `emb-deviation-review` attestation |

After the source lands, reinstall the changed policy so the repository
enforces the new build:

```sh
tools/build-policies.sh emb_<name>
nool admin plugin install policy-src/emb_<name>/emb_<name>.toml --policy
```

## Reporting a false positive or a miss

Open an issue with the smallest code that shows it, the rule id, the profile
values that matter, and what you expected. The smallest code becomes the test
case, so the fix cannot regress. If the pack refused correct code and you need
to keep working, deviate the line with the reason and reference the issue.

## Review checklist

- [ ] The clause was checked against the text; the source is in the catalog.
- [ ] The test is named after the rule id and shows the clause in the denial.
- [ ] There is a near miss that does not fire.
- [ ] Test and third-party paths are exempt (or the rule is about tests).
- [ ] Malformed input cannot panic.
- [ ] The class (Mandatory / Required / strict) is justified.
- [ ] The guide's table, "How it decides" and "Verify yourself" are updated.
- [ ] `tools/check.sh` ends with `ALL CHECKS PASS`.
