//! Aram policy: analysis suppressions, requirement traces, disabled tests, malformed deviations, weakened analysis config, and independent approval of deviations and hardware-profile changes before push. Rules: EMB-PRC-001..007.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_process;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbSafetyProcess;

policy!(EmbSafetyProcess);

impl Policy for EmbSafetyProcess {
    fn name() -> &'static str {
        "emb_safety_process"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_process(ctx)
    }
}
