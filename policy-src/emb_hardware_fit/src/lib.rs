//! Aram policy: code must fit the hardware in hardware-profile.toml — RAM
//! (stack objects), FPU, flash size, heap, and the execution model
//! (superloop, RTOS or event-driven). Rules: EMB-HW-001..006.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbHardwareFit;

policy!(EmbHardwareFit);

impl Policy for EmbHardwareFit {
    fn name() -> &'static str {
        "emb_hardware_fit"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::Hardware)
    }
}
