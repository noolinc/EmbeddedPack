//! `emb_safety_process`: the evidence a safety process depends on —
//! analysis left switched on, requirement traces, tests left enabled,
//! well-formed deviations, and independent approval of deviations and of
//! hardware-profile changes before a knot is pushed.
//!
//! Clauses were checked against MISRA Compliance:2020 (4.1, 4.2), NASA
//! NPR 7150.2D (3.12.1 [SWE-052], 4.5.3 [SWE-066], [SWE-185]), the CWE-699
//! data set and the published Power of 10 rules.

use crate::engine::{Sink, deviatable, rule_by_id};
use crate::hardware::PROFILE_FILE;
use crate::lex::Kind;
use crate::model::{Category, Lang, MIN_REASON, Pack, Rule, deviations, file_name, is_test_path, is_third_party, lang_of};
use crate::profile::Profile;

macro_rules! rules {
    ($( $name:ident = $id:literal, $cat:ident, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Process, category: Category::$cat, strict: false, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    SUPPRESSED = "EMB-PRC-001", Required, "analysis findings shall not be suppressed without a deviation",
        "MISRA Compliance:2020 4.1; Power of 10 Rule 10; CWE-1127; NASA NPR 7150.2D [SWE-185]";
    TRACE = "EMB-PRC-002", Required, "safety-classified source shall trace to a requirement",
        "NASA NPR 7150.2D 3.12.1 [SWE-052]; hardware profile: safety";
    TEST_DISABLED = "EMB-PRC-003", Required, "tests shall not be disabled or skipped",
        "NASA NPR 7150.2D 4.5.3 [SWE-066]";
    BAD_DEVIATION = "EMB-PRC-004", Mandatory, "a deviation shall name a deviatable rule and give its reason",
        "MISRA Compliance:2020 4.2";
    DEVIATION_APPROVAL = "EMB-PRC-005", Mandatory, "a deviation shall be authorized by an independent reviewer before push",
        "MISRA Compliance:2020 4.1";
    ANALYSIS_CONFIG = "EMB-PRC-006", Required, "the static-analysis configuration shall not be weakened",
        "Power of 10 Rule 10; MISRA Compliance:2020 4.1";
    PROFILE_APPROVAL = "EMB-PRC-007", Mandatory, "a hardware-profile change shall be reviewed before push",
        "hardware profile";
}

/// The attestation that authorizes the deviations in a knot.
pub const DEVIATION_REVIEW: &str = "emb-deviation-review";
/// The attestation that authorizes a change to the hardware profile.
pub const PROFILE_REVIEW: &str = "emb-profile-review";

const SUPPRESSION_MARKS: [&str; 10] = [
    "NOLINT", "cppcheck-suppress", "coverity[", "PRQA S", "lint -e", "lint !e", "NOSONAR", "parasoft-suppress", "polyspace", "codeql[",
];
const TRACE_MARKS: [&str; 12] =
    ["@req", "@trace", "@satisfies", "\\req", "\\trace", "REQ-", "SRS-", "SWR-", "SYS-", "LLR-", "HLR-", "Implements:"];
const SKIP_CALLS: [&str; 7] =
    ["TEST_IGNORE", "TEST_IGNORE_MESSAGE", "IGNORE_TEST", "GTEST_SKIP", "ztest_test_skip", "TEST_SKIP", "SKIP_TEST"];
const SKIP_LINES: [&str; 5] = ["@pytest.mark.skip", "@unittest.skip", "pytest.skip(", "@pytest.mark.xfail", "self.skipTest("];
const GUARDED_CHECKS: [&str; 5] = ["-*", "-cert-*", "-bugprone-*", "-clang-analyzer-*", "-misra-*"];

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if is_third_party(cx.path) || cx.lang == Lang::Other {
        return;
    }
    let test = is_test_path(cx.path);
    if cx.is_c_family() {
        if !test {
            suppressions(s);
            trace(s);
        }
        skipped_c(s, test);
    }
    if cx.lang == Lang::Script {
        for (n, line) in cx.text.lines().enumerate() {
            let l = line.trim_start();
            if let Some(m) = SKIP_LINES.iter().find(|m| l.starts_with(**m) || (!l.starts_with('#') && l.contains(**m))) {
                s.hit(&TEST_DISABLED, n as u32 + 1, format!("{m} skips a test"));
            }
        }
    }
    if file_name(cx.path) == ".clang-tidy" {
        clang_tidy(s);
    }
    if !test {
        for d in deviations(cx.text) {
            let why = match rule_by_id(d.rule_id) {
                None => format!("EMB-DEVIATION({}) names no rule of this pack", d.rule_id),
                Some(r) if !deviatable(r, cx.profile) => format!("{} cannot be deviated on this profile", r.id),
                Some(r) if d.reason.len() < MIN_REASON => format!("the deviation of {} needs a reason of at least {MIN_REASON} characters", r.id),
                Some(_) => continue,
            };
            s.hit(&BAD_DEVIATION, d.line, why);
        }
    }
}

fn suppressions(s: &mut Sink) {
    let cx = s.cx;
    for c in cx.comments() {
        if c.text.contains(crate::model::DEVIATION_MARK) {
            continue;
        }
        if let Some(m) = SUPPRESSION_MARKS.iter().find(|m| c.text.contains(**m)) {
            s.hit(&SUPPRESSED, c.line, format!("`{m}` suppresses an analysis finding"));
        }
    }
    let t = cx.toks();
    for i in 0..t.len() {
        if !(t[i].kind == Kind::Hash && t[i].pp && t.get(i + 1).is_some_and(|n| n.ident() == Some("pragma") && n.line == t[i].line)) {
            continue;
        }
        let line = t[i].line;
        let words: Vec<&str> = t[i + 2..].iter().take_while(|x| x.pp && x.line == line).map(|x| x.text).collect();
        let has = |w: &str| words.iter().any(|x| *x == w);
        if (has("diagnostic") && has("ignored")) || (has("warning") && has("disable")) || has("system_header") {
            s.hit(&SUPPRESSED, line, "#pragma switches compiler diagnostics off");
        }
    }
}

fn trace(s: &mut Sink) {
    let cx = s.cx;
    if !cx.profile.needs_traceability() || cx.funcs.is_empty() {
        return;
    }
    if !cx.comments().iter().any(|c| TRACE_MARKS.iter().any(|m| c.text.contains(m))) {
        s.hit(&TRACE, 1, "no requirement trace tag (for example `@req SRS-123`) in a file that defines functions");
    }
}

fn skipped_c(s: &mut Sink, test: bool) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.kind == Kind::Ident && !tok.pp {
            if tok.text.starts_with("DISABLED_") {
                s.hit(&TEST_DISABLED, tok.line, format!("{} is a disabled test", tok.text));
            } else if SKIP_CALLS.contains(&tok.text) && cx.is_call(i) {
                s.hit(&TEST_DISABLED, tok.line, format!("{}() skips a test", tok.text));
            }
        }
        if test && tok.kind == Kind::Hash && tok.pp {
            let same = |k: usize| t.get(i + k).filter(|x| x.pp && x.line == tok.line);
            if same(1).is_some_and(|x| x.ident() == Some("if")) && same(2).is_some_and(|x| x.text == "0") && same(3).is_none() {
                s.hit(&TEST_DISABLED, tok.line, "#if 0 removes test code");
            }
        }
    }
}

fn clang_tidy(s: &mut Sink) {
    let cx = s.cx;
    let is_key = |l: &str| l.split_once(':').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric()));
    let mut in_checks = false;
    let mut positive = false;
    let mut all_off = 0u32;
    for (n, raw) in cx.text.lines().enumerate() {
        let at = n as u32 + 1;
        if raw.trim_start().starts_with('#') {
            continue;
        }
        let mut body = raw;
        if is_key(raw) {
            let (key, value) = raw.split_once(':').unwrap_or((raw, ""));
            in_checks = key == "Checks";
            body = value;
            if key == "WarningsAsErrors" && value.trim().trim_matches(['\'', '"']).is_empty() {
                s.hit(&ANALYSIS_CONFIG, at, "WarningsAsErrors is empty: findings no longer fail the build");
            }
        }
        if !in_checks {
            continue;
        }
        for item in body.split(',') {
            let item = item.trim().trim_matches(['\'', '"', '>', '|']).trim();
            if item.is_empty() {
                continue;
            }
            if item == "-*" {
                all_off = at;
            } else if GUARDED_CHECKS.contains(&item) {
                s.hit(&ANALYSIS_CONFIG, at, format!("{item} switches a whole check group off"));
            } else if !item.starts_with('-') {
                positive = true;
            }
        }
    }
    if all_off != 0 && !positive {
        s.hit(&ANALYSIS_CONFIG, all_off, "-* with nothing enabled after it switches every check off");
    }
}

/// An approval a knot must carry before it is pushed.
#[derive(Debug)]
pub struct Approval {
    pub rule: &'static Rule,
    pub assertion: &'static str,
    pub detail: String,
}

/// The approvals the changed files call for. Deviations are counted where
/// they take effect: production files, and `EMB-PRC-003` in test files.
pub fn approvals_needed(files: &[(&str, &str)], profile: &Profile) -> Vec<Approval> {
    let mut out = Vec::new();
    let mut devs = 0usize;
    let mut first = String::new();
    for (path, text) in files {
        if file_name(path) == PROFILE_FILE {
            out.push(Approval { rule: &PROFILE_APPROVAL, assertion: PROFILE_REVIEW, detail: format!("{path} changes the hardware profile") });
        }
        if is_third_party(path) || lang_of(path) == Lang::Other {
            continue;
        }
        let test = is_test_path(path);
        for d in deviations(text) {
            let known = rule_by_id(d.rule_id).is_some_and(|r| deviatable(r, profile));
            if known && (!test || d.rule_id == TEST_DISABLED.id) {
                if devs == 0 {
                    first = format!("{path}:{} deviates {}", d.line, d.rule_id);
                }
                devs += 1;
            }
        }
    }
    if devs > 0 {
        let more = if devs > 1 { format!(" (+{} more)", devs - 1) } else { String::new() };
        out.push(Approval { rule: &DEVIATION_APPROVAL, assertion: DEVIATION_REVIEW, detail: format!("{first}{more}") });
    }
    out
}

/// The pending-evidence message for approvals that are still missing.
pub fn render_pending(missing: &[Approval]) -> (String, String) {
    let line = |a: &Approval| {
        format!(
            "[{}] {} ({}) -- {} | waiting for a trusted `{}` pass: nool attest record --target <knot> --checker <reviewer> --assertion {} --verdict pass",
            a.rule.id, a.detail, a.rule.title, a.rule.refs, a.assertion, a.assertion
        )
    };
    let message = missing.first().map(line).unwrap_or_default();
    let evidence = missing.iter().map(line).collect::<Vec<_>>().join("\n");
    (message, evidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{check, render};

    fn asil_b() -> Profile {
        Profile::parse("[safety]\niso26262_asil = \"B\"").unwrap()
    }
    fn qm() -> Profile {
        Profile::default()
    }
    fn ids_with(p: &Profile, path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Process, &[(path, src)], p).iter().map(|f| f.rule.id).collect()
    }
    fn ids_at(path: &str, src: &str) -> Vec<&'static str> {
        ids_with(&qm(), path, src)
    }
    fn rule(id: &str, clause: &str, path: &str, bad: &str, good: &str) {
        let p = asil_b();
        let f = check(Pack::Process, &[(path, bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids_with(&p, path, bad));
        assert!(render(&f, &p).1.contains(clause));
        assert!(!ids_with(&p, path, good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn prc001_analysis_suppressed() {
        rule(
            "EMB-PRC-001",
            "MISRA Compliance:2020 4.1; Power of 10 Rule 10; CWE-1127; NASA NPR 7150.2D [SWE-185]",
            "src/a.c",
            "/* @req SRS-1 */\nint f(int a) { return a << 40; } // NOLINT\n",
            "/* @req SRS-1 */\nint f(int a) { return a; }\n",
        );
        for bad in [
            "#pragma GCC diagnostic ignored \"-Wconversion\"\n",
            "#pragma warning(disable: 4996)\n",
            "// cppcheck-suppress nullPointer\nint x;\n",
            "/* coverity[overrun-local] */\nint x;\n",
            "int x; /* PRQA S 0306 */\n",
            "int x; //lint -e{9027}\n",
        ] {
            assert_eq!(ids_at("src/a.c", bad), ["EMB-PRC-001"], "{bad}");
        }
        // A justified deviation covers the suppression on the next line.
        let ok = "/* EMB-DEVIATION(EMB-PRC-001): vendor header triggers -Wpedantic, DR-12 */\n#pragma GCC diagnostic ignored \"-Wpedantic\"\n";
        assert!(ids_at("src/a.c", ok).is_empty());
        assert!(ids_at("src/a.c", "#pragma once\n#pragma pack(push, 1)\nconst char *s = \"NOLINT\";\n").is_empty());
    }
    #[test]
    fn prc002_requirement_trace() {
        rule(
            "EMB-PRC-002",
            "NASA NPR 7150.2D 3.12.1 [SWE-052]",
            "src/brake.c",
            "int brake_force(int p) { return p * 2; }\n",
            "/** @req SRS-BRK-014 */\nint brake_force(int p) { return p * 2; }\n",
        );
        // No safety level, or nothing but declarations: no trace demanded.
        assert!(ids_with(&qm(), "src/brake.c", "int brake_force(int p) { return p * 2; }\n").is_empty());
        assert!(ids_with(&asil_b(), "src/brake.h", "int brake_force(int p);\n").is_empty());
        assert!(ids_with(&Profile::parse("[safety]\niec62304_class = \"A\"").unwrap(), "src/a.c", "int f(void) { return 1; }\n").is_empty());
        assert_eq!(ids_with(&Profile::parse("[safety]\niec62304_class = \"B\"").unwrap(), "src/a.c", "int f(void) { return 1; }\n"), ["EMB-PRC-002"]);
    }
    #[test]
    fn prc003_test_disabled() {
        rule(
            "EMB-PRC-003",
            "NASA NPR 7150.2D 4.5.3 [SWE-066]",
            "tests/test_brake.cpp",
            "TEST(Brake, DISABLED_Overheat) { EXPECT_EQ(1, f()); }\n",
            "TEST(Brake, Overheat) { EXPECT_EQ(1, f()); }\n",
        );
        for (path, bad) in [
            ("tests/test_a.c", "void test_x(void) { TEST_IGNORE(); }\n"),
            ("tests/test_a.c", "void test_x(void) { TEST_IGNORE_MESSAGE(\"flaky\"); }\n"),
            ("tests/test_a.cpp", "TEST(A, B) { GTEST_SKIP(); }\n"),
            ("tests/test_a.c", "#if 0\nvoid test_x(void) { }\n#endif\n"),
            ("tests/test_a.py", "@pytest.mark.skip(reason=\"later\")\ndef test_x():\n    pass\n"),
            ("tests/main.c", "ZTEST(s, test_x) { ztest_test_skip(); }\n"),
        ] {
            assert_eq!(ids_at(path, bad), ["EMB-PRC-003"], "{bad}");
        }
        assert!(ids_at("tests/test_a.c", "#if 0 == FEATURE\nint x;\n#endif\n#if 1\nint y;\n#endif\n").is_empty());
        assert!(ids_at("tests/test_a.py", "# @pytest.mark.skip was removed\ndef test_x():\n    pass\n").is_empty());
    }
    #[test]
    fn prc004_malformed_deviation() {
        rule(
            "EMB-PRC-004",
            "MISRA Compliance:2020 4.2",
            "src/a.c",
            "/* @req SRS-1 */\n/* EMB-DEVIATION(EMB-C-001): needed */\nint x;\n",
            "/* @req SRS-1 */\n/* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */\nint x;\n",
        );
        assert_eq!(ids_at("src/a.c", "/* EMB-DEVIATION(EMB-C-999): this rule does not exist anywhere */\n"), ["EMB-PRC-004"]);
        // Mandatory rules, and strict rules on a high-integrity profile.
        assert_eq!(ids_at("src/a.c", "/* EMB-DEVIATION(EMB-SEC-001): the password is only a placeholder */\n"), ["EMB-PRC-004"]);
        let d = "/* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */\n";
        assert_eq!(ids_with(&Profile::parse("[safety]\niso26262_asil = \"D\"").unwrap(), "include/a.h", d), ["EMB-PRC-004"]);
        // It cannot waive itself.
        assert_eq!(ids_at("src/a.c", "/* EMB-DEVIATION(EMB-PRC-004): trust me this is fine really */\n"), ["EMB-PRC-004"]);
        assert_eq!(ids_at("prj.conf", "# EMB-DEVIATION(EMB-FWI-004): no\nCONFIG_X=y\n"), ["EMB-PRC-004"]);
    }
    #[test]
    fn prc005_deviation_needs_approval() {
        let p = qm();
        let src = "void f(void) {\n  /* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */\n  goto out;\nout: return;\n}\n";
        let need = approvals_needed(&[("src/a.c", src), ("src/b.c", src)], &p);
        assert_eq!(need.len(), 1);
        assert_eq!((need[0].rule.id, need[0].assertion), ("EMB-PRC-005", "emb-deviation-review"));
        assert!(need[0].detail.contains("src/a.c:2 deviates EMB-C-001 (+1 more)"), "{}", need[0].detail);
        let (msg, ev) = render_pending(&need);
        assert!(msg.contains("MISRA Compliance:2020 4.1") && ev.contains("--assertion emb-deviation-review --verdict pass"));
    }
    #[test]
    fn prc006_analysis_config_weakened() {
        rule(
            "EMB-PRC-006",
            "Power of 10 Rule 10; MISRA Compliance:2020 4.1",
            ".clang-tidy",
            "Checks: '-*'\n",
            "Checks: '-*,cert-*,bugprone-*,clang-analyzer-*'\nWarningsAsErrors: '*'\n",
        );
        assert_eq!(ids_at(".clang-tidy", "Checks: >\n  bugprone-*,\n  -cert-*,\n  -clang-analyzer-*\n"), ["EMB-PRC-006", "EMB-PRC-006"]);
        assert_eq!(ids_at(".clang-tidy", "Checks: 'cert-*'\nWarningsAsErrors: ''\n"), ["EMB-PRC-006"]);
        assert!(ids_at(".clang-tidy", "# Checks: '-*'\nChecks: 'cert-*,-cert-err58-cpp'\nCheckOptions:\n  - key: a\n    value: '-*'\n").is_empty());
    }
    #[test]
    fn prc007_profile_change_needs_approval() {
        let need = approvals_needed(&[("hardware-profile.toml", "[hardware]\nram_bytes = 1024\n")], &qm());
        assert_eq!(need.len(), 1);
        assert_eq!((need[0].rule.id, need[0].assertion), ("EMB-PRC-007", "emb-profile-review"));
    }

    #[test]
    fn prc_does_not_flag_compliant_files() {
        let p = asil_b();
        let src = "/**\n * @file brake.c\n * @req SRS-BRK-014\n */\n#include \"brake.h\"\n#pragma pack(push, 1)\nstruct frame { unsigned char id; };\n#pragma pack(pop)\n/* EMB-DEVIATION(EMB-PRC-001): third-party macro expands to a cast, DR-3 */\nint brake_force(int p) { return VENDOR(p); } // NOLINT(google-readability-casting)\n";
        assert_eq!(ids_with(&p, "src/brake.c", src), Vec::<&str>::new());
        assert_eq!(ids_with(&p, "tests/test_brake.c", "void test_force(void) { TEST_ASSERT_EQUAL(4, brake_force(2)); }\n"), Vec::<&str>::new());
        assert_eq!(ids_with(&p, ".clang-tidy", "Checks: 'cert-*,bugprone-*,clang-analyzer-*,-bugprone-easily-swappable-parameters'\nWarningsAsErrors: '*'\n"), Vec::<&str>::new());
        assert_eq!(ids_with(&p, "third_party/lib/x.c", "int f(void) { return 1; } // NOLINT\n"), Vec::<&str>::new());
        assert_eq!(ids_with(&p, "docs/dev.md", "EMB-DEVIATION(EMB-X): no\n"), Vec::<&str>::new());
    }
    #[test]
    fn prc_approvals_needed() {
        let p = qm();
        // Nothing to approve.
        assert!(approvals_needed(&[("src/a.c", "int f(void) { return 1; }\n"), ("README.md", "EMB-DEVIATION(EMB-C-001): shown as an example only\n")], &p).is_empty());
        // Markers that waive nothing are not approvals: unknown, mandatory, third-party, or in a test for a production rule.
        let d = "/* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */\n";
        assert!(approvals_needed(&[("third_party/a.c", d), ("tests/test_a.c", d), ("src/b.c", "/* EMB-DEVIATION(EMB-SEC-001): placeholder password only */\n")], &p).is_empty());
        // A skipped test with a deviation does need one.
        let t = "/* EMB-DEVIATION(EMB-PRC-003): hardware-in-loop rig offline until DR-21 */\nvoid test_x(void) { TEST_IGNORE(); }\n";
        assert_eq!(approvals_needed(&[("tests/test_a.c", t)], &p).len(), 1);
        // Both kinds at once.
        let both = approvals_needed(&[("hardware-profile.toml", ""), ("src/a.c", d)], &p);
        assert_eq!(both.iter().map(|a| a.assertion).collect::<Vec<_>>(), ["emb-profile-review", "emb-deviation-review"]);
    }
    #[test]
    fn malformed_prc_input_never_panics() {
        for src in ["#pragma", "#", "#if", "EMB-DEVIATION(", "EMB-DEVIATION()", "Checks:", ":", "Checks: ,,,'", "TEST(", "é // NOLINT ü"] {
            for path in ["a.c", "tests/a_test.cpp", ".clang-tidy", "a.py", "a.conf"] {
                let _ = ids_with(&asil_b(), path, src);
                let _ = approvals_needed(&[(path, src)], &asil_b());
            }
        }
    }
}
