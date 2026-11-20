//! Runs a pack over changed files, applies deviations, renders the denial.

use crate::model::{Category, FileCtx, Finding, Pack, Rule, deviations};
use crate::profile::Profile;
use crate::{c_subset, cpp_subset, hardware, resources, runtime, security};

/// Every rule in every pack.
pub fn all_rules() -> Vec<&'static Rule> {
    let mut v: Vec<&'static Rule> = Vec::new();
    v.extend(c_subset::RULES.iter());
    v.extend(cpp_subset::RULES.iter());
    v.extend(resources::RULES.iter());
    v.extend(hardware::RULES.iter());
    v.extend(security::RULES.iter());
    v.extend(runtime::RULES.iter());
    v
}

pub fn rule_by_id(id: &str) -> Option<&'static Rule> {
    all_rules().into_iter().find(|r| r.id == id)
}

/// Whether a deviation comment may waive `rule` on this profile.
pub fn deviatable(rule: &Rule, profile: &Profile) -> bool {
    rule.category == Category::Required && !(rule.strict && profile.high_integrity())
}

/// Collects findings for one file.
pub struct Sink<'c, 'a> {
    pub cx: &'c FileCtx<'a>,
    pub out: Vec<Finding>,
}

impl Sink<'_, '_> {
    pub fn hit(&mut self, rule: &'static Rule, line: u32, detail: impl Into<String>) {
        self.out.push(Finding { rule, path: self.cx.path.to_string(), line, detail: detail.into() });
    }
}

/// Files larger than this are not analysed: a policy has a fixed fuel budget.
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

fn check_file(pack: Pack, path: &str, text: &str, profile: &Profile) -> Vec<Finding> {
    if text.len() > MAX_FILE_BYTES || text.as_bytes().iter().take(4096).any(|b| *b == 0) {
        return Vec::new();
    }
    let cx = FileCtx::new(path, text, profile);
    let mut sink = Sink { cx: &cx, out: Vec::new() };
    match pack {
        Pack::CSubset => c_subset::run(&mut sink),
        Pack::CppSubset => cpp_subset::run(&mut sink),
        Pack::Resources => resources::run(&mut sink),
        Pack::Hardware => hardware::run(&mut sink),
        Pack::Security => security::run(&mut sink),
        Pack::Runtime => runtime::run(&mut sink),
        _ => {}
    }
    let devs = deviations(text);
    let mut out = sink.out;
    out.retain(|f| {
        !(deviatable(f.rule, profile)
            && devs.iter().any(|d| {
                d.rule_id == f.rule.id
                    && d.reason.len() >= crate::model::MIN_REASON
                    && (f.line == 0 || d.line == f.line || d.line + 1 == f.line)
            }))
    });
    out.sort_by_key(|f| f.line);
    out
}

/// Findings for `pack` over the changed files (`(path, new content)`).
pub fn check(pack: Pack, files: &[(&str, &str)], profile: &Profile) -> Vec<Finding> {
    files.iter().flat_map(|(p, t)| check_file(pack, p, t, profile)).collect()
}

/// How many findings a denial spells out; the rest are counted.
pub const MAX_SHOWN: usize = 12;

/// The denial message (one line, first finding) and the evidence (the list).
pub fn render(findings: &[Finding], profile: &Profile) -> (String, String) {
    let Some(first) = findings.first() else {
        return (String::new(), String::new());
    };
    let line = |f: &Finding| {
        let at = if f.line == 0 { f.path.clone() } else { format!("{}:{}", f.path, f.line) };
        format!("[{}] {at}: {} ({}) -- {}", f.rule.id, f.detail, f.rule.title, f.rule.refs)
    };
    let more = findings.len() - 1;
    let message = if more == 0 { line(first) } else { format!("{} (+{more} more)", line(first)) };
    let mut evidence = String::new();
    for f in findings.iter().take(MAX_SHOWN) {
        evidence.push_str(&line(f));
        if deviatable(f.rule, profile) {
            evidence.push_str(&format!(" | deviate: /* EMB-DEVIATION({}): <reason> */ on or above the line", f.rule.id));
        } else {
            evidence.push_str(" | not deviatable");
        }
        evidence.push('\n');
    }
    if findings.len() > MAX_SHOWN {
        evidence.push_str(&format!("... and {} more\n", findings.len() - MAX_SHOWN));
    }
    evidence.push_str(&format!("profile: {}", profile.summary()));
    (message, evidence)
}
