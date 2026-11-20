//! Bridges `emb-rules` to the Nool Aram policy SDK: pulls the changed files
//! out of a knot, runs one pack, and turns findings into a denial.

use emb_rules::{Pack, Profile};
use nool_aram_policy_sdk::bindings::LifecycleStage;
use nool_aram_policy_sdk::{PolicyContext, PolicyResult, reason_codes};
use nool_policy_abi::models::KnotPayload;

/// The profile this build enforces.
pub const PROFILE_TOML: &str = include_str!("../../../hardware-profile.toml");

/// Changed files as `(path, new content)`. A multi-file change is a
/// `Synthesis` of mutations; nesting is walked with an explicit stack.
pub fn changed_files<'k>(payload: &'k KnotPayload<'k>) -> Vec<(&'k str, &'k [u8])> {
    let mut out = Vec::new();
    let mut stack = vec![payload];
    while let Some(p) = stack.pop() {
        match p {
            KnotPayload::Mutation { semantic_path, diff, .. } => out.push((semantic_path.as_str(), diff.as_ref())),
            KnotPayload::Synthesis(parts) => stack.extend(parts.iter().map(|s| s.payload.as_ref())),
            _ => {}
        }
    }
    out.reverse();
    out
}

/// Evaluate one pack. Runs when a change is proposed and sealed; at push the
/// knot is already sealed under the same rules, so there is nothing to add.
pub fn evaluate_pack(ctx: &PolicyContext, pack: Pack) -> PolicyResult {
    if matches!(ctx.stage, Some(LifecycleStage::PrePush)) {
        return PolicyResult::Allow;
    }
    let files = changed_files(&ctx.knot.transform.payload);
    if files.is_empty() {
        return PolicyResult::Allow;
    }
    let profile = match Profile::parse(PROFILE_TOML) {
        Ok(p) => p,
        Err(e) => {
            return PolicyResult::deny(
                reason_codes::INVARIANT_VIOLATION,
                format!("this policy was built with an invalid hardware-profile.toml ({e}); fix it and rebuild the policies"),
            );
        }
    };
    let texts: Vec<(&str, std::borrow::Cow<str>)> = files.iter().map(|(p, b)| (*p, String::from_utf8_lossy(b))).collect();
    let refs: Vec<(&str, &str)> = texts.iter().map(|(p, t)| (*p, t.as_ref())).collect();
    let findings = emb_rules::check(pack, &refs, &profile);
    if findings.is_empty() {
        return PolicyResult::Allow;
    }
    let (message, evidence) = emb_rules::render(&findings, &profile);
    PolicyResult::deny_with_evidence(reason_codes::INVARIANT_VIOLATION, message, evidence)
}
