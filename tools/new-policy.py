#!/usr/bin/env python3
"""Scaffold a new policy and register its rule module.

    tools/new-policy.py <module> <Pack> <policy_name> "<one-line description>"
    tools/new-policy.py timing Timing emb_timing "deadline and timer rules. Rules: EMB-TIM-001.."

Before running it: add the `Pack::<Pack>` variant to crates/emb-rules/src/model.rs
and write crates/emb-rules/src/<module>.rs with `pub static RULES` and `pub fn run`.
It registers the module in lib.rs and engine.rs and creates policy-src/<policy_name>/.
"""
import os, re, sys

if len(sys.argv) != 5:
    sys.exit(__doc__)
mod, pack, name, doc = sys.argv[1:5]
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
if not os.path.exists(f"crates/emb-rules/src/{mod}.rs"):
    sys.exit(f"write crates/emb-rules/src/{mod}.rs first")
struct = "".join(w.capitalize() for w in name.split("_"))

p = "crates/emb-rules/src/engine.rs"
s = open(p).read()
m = re.search(r"use crate::\{([^}]*)\};", s)
mods = sorted(set(x.strip() for x in m.group(1).split(",")) | {mod})
s = s.replace(m.group(0), "use crate::{%s};" % ", ".join(mods))
if f"{mod}::RULES" not in s:
    s = s.replace("    v\n}", f"    v.extend({mod}::RULES.iter());\n    v\n}}", 1)
    s = s.replace("    }\n    let devs", f"        Pack::{pack} => {mod}::run(&mut sink),\n    }}\n    let devs", 1)
open(p, "w").write(s)

p = "crates/emb-rules/src/lib.rs"
lines = open(p).read().split("\n")
at = [k for k, l in enumerate(lines) if l.startswith("pub mod ")]
lines[at[0]:at[-1] + 1] = sorted(set(lines[at[0]:at[-1] + 1]) | {f"pub mod {mod};"})
open(p, "w").write("\n".join(lines))

d = f"policy-src/{name}"
os.makedirs(d + "/src", exist_ok=True)
for src, dst in (("Cargo.toml", "Cargo.toml"), ("emb_c_subset.toml", f"{name}.toml")):
    open(f"{d}/{dst}", "w").write(open(f"policy-src/emb_c_subset/{src}").read().replace("emb_c_subset", name))
open(d + "/src/lib.rs", "w").write(f'''//! Aram policy: {doc}
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{{Policy, PolicyContext, PolicyResult, policy}};

struct {struct};

policy!({struct});

impl Policy for {struct} {{
    fn name() -> &'static str {{
        "{name}"
    }}

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {{
        evaluate_pack(ctx, emb_rules::Pack::{pack})
    }}
}}
''')
print(f"created {d}/ and registered {mod}. Next: tests/e2e/{name}.sh, then tools/check.sh {name}")
