//! Aram policy: build and boot configuration — compiler diagnostics and hardening, secure boot, update authenticity, rollback protection, signing keys, debug access. Rules: EMB-FWI-001..007.
#![allow(unsafe_op_in_unsafe_fn)]

use emb_policy_glue::evaluate_pack;
use nool_aram_policy_sdk::{Policy, PolicyContext, PolicyResult, policy};

struct EmbFirmwareIntegrity;

policy!(EmbFirmwareIntegrity);

impl Policy for EmbFirmwareIntegrity {
    fn name() -> &'static str {
        "emb_firmware_integrity"
    }

    fn evaluate(ctx: &PolicyContext) -> PolicyResult {
        evaluate_pack(ctx, emb_rules::Pack::Integrity)
    }
}
