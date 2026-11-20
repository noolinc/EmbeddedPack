//! Aram policy: the C++ language subset for production firmware
//! (AUTOSAR C++14, JSF AV C++, MISRA C++:2008 as adopted by AUTOSAR).
//! Rules: EMB-CPP-001..018.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbCppSubset;

policy!(EmbCppSubset);

impl Policy for EmbCppSubset {
    fn name() -> &'static str {
        "emb_cpp_subset"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::CppSubset)
    }
}
