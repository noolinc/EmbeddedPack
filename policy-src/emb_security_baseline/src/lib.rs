//! Aram policy: device security baseline — hard-coded credentials and keys,
//! weak cryptography and randomness, unbounded string functions, disabled
//! certificate verification, cleartext endpoints, format strings, timing
//! leaks and command execution. Rules: EMB-SEC-001..011.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbSecurityBaseline;

policy!(EmbSecurityBaseline);

impl Policy for EmbSecurityBaseline {
    fn name() -> &'static str {
        "emb_security_baseline"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::Security)
    }
}
