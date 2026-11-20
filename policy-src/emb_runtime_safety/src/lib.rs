//! Aram policy: interrupt service routines, critical sections, non-reentrant library calls and the watchdog. Rules: EMB-RT-001..005.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbRuntimeSafety;

policy!(EmbRuntimeSafety);

impl Policy for EmbRuntimeSafety {
    fn name() -> &'static str {
        "emb_runtime_safety"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::Runtime)
    }
}
