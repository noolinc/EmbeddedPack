//! Aram policy: the C language subset for production firmware
//! (MISRA C:2012, BARR-C:2018, SEI CERT C, Power of 10). Rules: EMB-C-001..019.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbCSubset;

policy!(EmbCSubset);

impl Policy for EmbCSubset {
    fn name() -> &'static str {
        "emb_c_subset"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::CSubset)
    }
}
