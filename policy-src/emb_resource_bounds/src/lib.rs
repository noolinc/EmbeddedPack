//! Aram policy: bounded memory and execution (heap per the hardware profile,
//! recursion, alloca/VLA, loop bounds, function length, pointer depth).
//! Rules: EMB-RES-001..006.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbResourceBounds;

policy!(EmbResourceBounds);

impl Policy for EmbResourceBounds {
    fn name() -> &'static str {
        "emb_resource_bounds"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::Resources)
    }
}
