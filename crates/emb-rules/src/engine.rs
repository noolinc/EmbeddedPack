//! Runs a pack over changed files, applies deviations, renders the denial.

use crate::model::{Category, ChangeInfo, FileCtx, Finding, Pack, Rule, deviations};
use crate::profile::Profile;
use crate::{c_subset, cpp_subset, hardware, integrity, process, resources, runtime, security};

/// Every rule in every pack.
pub fn all_rules() -> Vec<&'static Rule> {
    let mut v: Vec<&'static Rule> = Vec::new();
    v.extend(c_subset::RULES.iter());
    v.extend(cpp_subset::RULES.iter());
    v.extend(resources::RULES.iter());
    v.extend(hardware::RULES.iter());
    v.extend(security::RULES.iter());
    v.extend(runtime::RULES.iter());
    v.extend(integrity::RULES.iter());
    v.extend(process::RULES.iter());
    v
}

pub fn rule_by_id(id: &str) -> Option<&'static Rule> {
    all_rules().into_iter().find(|r| r.id == id)
}

/// Whether a deviation comment may waive `rule` on this profile.
pub fn deviatable(rule: &Rule, profile: &Profile) -> bool {
    profile.category_of(rule) == Some(Category::Required) && !(rule.strict && profile.high_integrity())
}

/// Whether `f` only warns under this profile.
pub fn is_advisory(f: &Finding, profile: &Profile) -> bool {
    profile.category_of(f.rule) == Some(Category::Advisory)
}

/// Drop disapplied rules and findings a justified deviation in `text` waives.
fn waive(found: Vec<Finding>, text_of: &dyn Fn(&str) -> Option<String>, profile: &Profile) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in found {
        if profile.category_of(f.rule).is_none() {
            continue;
        }
        let waived = deviatable(f.rule, profile)
            && text_of(&f.path).is_some_and(|text| {
                deviations(&text).iter().any(|d| {
                    d.rule_id == f.rule.id
                        && d.reason.len() >= crate::model::MIN_REASON
                        && (f.line == 0 || d.line == f.line || d.line + 1 == f.line)
                })
            });
        if !waived {
            out.push(f);
        }
    }
    out
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

fn check_file(pack: Pack, path: &str, text: &str, profile: &Profile) -> Vec<Finding> {
    if text.len() as u64 > profile.max_file_bytes || text.as_bytes().iter().take(4096).any(|b| *b == 0) {
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
        Pack::Integrity => integrity::run(&mut sink),
        Pack::Process => process::run(&mut sink),
    }
    let mut out = waive(sink.out, &|_| Some(text.to_string()), profile);
    out.sort_by_key(|f| f.line);
    out
}

/// Findings for `pack` over the changed files (`(path, new content)`),
/// with nothing known about the change itself.
pub fn check(pack: Pack, files: &[(&str, &str)], profile: &Profile) -> Vec<Finding> {
    check_change(pack, files, profile, &ChangeInfo::unknown())
}

/// Findings for `pack`: every file on its own, then the rules that judge
/// the change as a whole (what it declares, which files it combines).
pub fn check_change(pack: Pack, files: &[(&str, &str)], profile: &Profile, change: &ChangeInfo) -> Vec<Finding> {
    let mut out: Vec<Finding> = files.iter().flat_map(|(p, t)| check_file(pack, p, t, profile)).collect();
    let whole = match pack {
        Pack::Integrity => integrity::run_change(files, profile, change),
        Pack::Process => process::run_change(files, profile, change),
        _ => Vec::new(),
    };
    let text_of = |path: &str| files.iter().find(|(p, _)| *p == path).map(|(_, t)| t.to_string());
    out.extend(waive(whole, &text_of, profile));
    out
}

/// Blocking findings first, advisory ones second.
pub fn split_advisory(findings: Vec<Finding>, profile: &Profile) -> (Vec<Finding>, Vec<Finding>) {
    findings.into_iter().partition(|f| !is_advisory(f, profile))
}

/// One advisory finding as a warning: (message, clause).
pub fn render_warning(f: &Finding) -> (String, String) {
    let at = if f.line == 0 { f.path.clone() } else { format!("{}:{}", f.path, f.line) };
    (format!("[{}] {at}: {} ({}) | advisory: identified, not blocking", f.rule.id, f.detail, f.rule.title), f.rule.refs.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    const GOTO: &str = "void f(void) { goto out; out: return; }\n";
    const CUBE: &str = "/* USER CODE BEGIN 0 */\nint x;\n";

    #[test]
    fn grp_permitted_moves_follow_misra_compliance_5_1() {
        // Required -> Mandatory: allowed, and then no deviation waives it.
        let p = Profile::parse("[recategorize]\n\"EMB-C-001\" = \"mandatory\"\n").unwrap();
        let dev = "void f(void) {\n/* EMB-DEVIATION(EMB-C-001): single cleanup exit, see DR-7 */\ngoto out; out: return; }\n";
        assert!(check(Pack::CSubset, &[("src/a.c", dev)], &Profile::default()).is_empty());
        assert_eq!(check(Pack::CSubset, &[("src/a.c", dev)], &p).len(), 1);
        // Advisory -> Required: blocks; Advisory -> Disapplied: gone entirely.
        let req = Profile::parse("[recategorize]\nEMB-PRC-011 = required\n").unwrap();
        let (block, warn) = split_advisory(check(Pack::Process, &[("src/main.c", CUBE)], &req), &req);
        assert_eq!((block.len(), warn.len()), (1, 0));
        let off = Profile::parse("[recategorize]\nEMB-PRC-011 = disapplied\n").unwrap();
        assert!(check(Pack::Process, &[("src/main.c", CUBE)], &off).is_empty());
    }
    #[test]
    fn grp_forbidden_moves_are_profile_errors() {
        for bad in [
            "[recategorize]\nEMB-C-001 = advisory\n",
            "[recategorize]\nEMB-C-001 = disapplied\n",
            "[recategorize]\nEMB-SEC-001 = required\n",
            "[recategorize]\nEMB-X-999 = mandatory\n",
            "[recategorize]\nEMB-C-001 = sometimes\n",
        ] {
            let e = Profile::parse(bad).unwrap_err();
            assert!(bad.contains("EMB-X-999") || bad.contains("sometimes") || e.contains("MISRA Compliance:2020 5.1"), "{bad}: {e}");
        }
    }
    #[test]
    fn advisory_findings_never_block() {
        let p = Profile::default();
        let f = check(Pack::Process, &[("src/main.c", CUBE)], &p);
        let (block, warn) = split_advisory(f, &p);
        assert!(block.is_empty());
        assert_eq!(warn.len(), 1);
        let (msg, clause) = render_warning(&warn[0]);
        assert!(msg.starts_with("[EMB-PRC-011] src/main.c:1:") && msg.contains("not blocking"), "{msg}");
        assert_eq!(clause, "NASA NPR 7150.2D 5.1.4 [SWE-081]");
        // A Required finding still blocks alongside it.
        let (block, _) = split_advisory(check(Pack::CSubset, &[("src/a.c", GOTO)], &p), &p);
        assert_eq!(block.len(), 1);
    }

    #[test]
    fn a_file_over_max_file_bytes_is_skipped_until_the_limit_is_raised() {
        let src = format!("void f(void) {{ goto out; out: return; }}\n{}", "/* pad */\n".repeat(200));
        let small = Profile { max_file_bytes: 64, ..Profile::default() };
        assert!(check(Pack::CSubset, &[("src/a.c", &src)], &small).is_empty());
        let large = Profile { max_file_bytes: src.len() as u64, ..Profile::default() };
        assert!(check(Pack::CSubset, &[("src/a.c", &src)], &large).iter().any(|f| f.rule.id == "EMB-C-001"));
    }
}
