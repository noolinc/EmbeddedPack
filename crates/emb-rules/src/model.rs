//! Rules, findings and the per-file context every check reads.

use crate::lex::{Comment, Kind, Lexed, NONE, Tok, lex};
use crate::profile::Profile;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    /// Never deviatable.
    Mandatory,
    /// Deviatable with an `EMB-DEVIATION(<id>): <reason>` comment, which then
    /// needs an independent approval before push. Not deviatable at all on a
    /// high-integrity profile when the rule is marked `strict`.
    Required,
    /// Identified, never blocking: the policy reports it as a warning and
    /// needs no deviation (MISRA Compliance:2020 5.1, note 3).
    Advisory,
}

/// What the change itself declares, from the host's intent projection.
/// `known` is false when the host supplied none (an older host, a pulled
/// knot): change-level rules then judge nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeInfo {
    pub known: bool,
    /// Proposed with `--breaking`.
    pub breaking: bool,
    /// `--issue` references.
    pub issues: String,
    /// The `--why` rationale.
    pub why: String,
}

impl ChangeInfo {
    pub fn unknown() -> Self {
        ChangeInfo::default()
    }
}

/// One policy module enforces one pack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pack {
    CSubset,
    CppSubset,
    Resources,
    Runtime,
    Hardware,
    Security,
    Integrity,
    Process,
}

#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub pack: Pack,
    pub category: Category,
    /// Becomes mandatory on a high-integrity profile (ASIL C/D, class C, SIL 3/4).
    pub strict: bool,
    pub title: &'static str,
    /// Clauses this rule enforces, `;`-separated.
    pub refs: &'static str,
}

#[derive(Debug)]
pub struct Finding {
    pub rule: &'static Rule,
    pub path: String,
    /// 0 = the file as a whole.
    pub line: u32,
    pub detail: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    C,
    Cpp,
    /// Makefile, CMake, Meson, PlatformIO, compiler flag files.
    Build,
    /// Kconfig fragments, sdkconfig, `.conf`, `.ini`, `.cfg`, YAML, JSON.
    Config,
    /// Shell, Python, Dockerfiles, CI definitions.
    Script,
    Other,
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub fn lang_of(path: &str) -> Lang {
    let name = file_name(path);
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if lower == "makefile"
        || lower == "gnumakefile"
        || lower == "cmakelists.txt"
        || lower == "meson.build"
        || lower == "platformio.ini"
        || lower == "compile_flags.txt"
        || lower == "sconstruct"
        || matches!(ext, "mk" | "mak" | "cmake" | "gn" | "gni" | "ninja")
    {
        return Lang::Build;
    }
    if lower == "dockerfile" || lower.starts_with("dockerfile.") || lower == "jenkinsfile" {
        return Lang::Script;
    }
    if lower.starts_with("sdkconfig") || lower == "kconfig" || lower.starts_with("kconfig.") || lower.ends_with("_defconfig") || lower == ".clang-tidy" {
        return Lang::Config;
    }
    match ext {
        "c" | "h" => Lang::C,
        "cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ino" | "ipp" | "tpp" => Lang::Cpp,
        "conf" | "cfg" | "ini" | "yaml" | "yml" | "json" | "overlay" | "dts" | "dtsi" | "defconfig" | "config" => Lang::Config,
        "sh" | "bash" | "py" | "ps1" | "bat" => Lang::Script,
        _ => Lang::Other,
    }
}

/// Test code and adopted third-party code are outside the language subset.
/// The test-integrity rule is the one check that looks only at test code.
pub fn is_test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let name = file_name(&p);
    p.split(['/', '\\']).any(|s| matches!(s, "test" | "tests" | "unittest" | "unittests" | "unit_tests" | "mocks" | "testing"))
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains("_tests.")
}

/// Linker scripts: where code and data sit in flash and RAM.
pub fn is_linker_script(path: &str) -> bool {
    let n = file_name(path).to_ascii_lowercase();
    [".ld", ".lds", ".icf", ".sct", ".scat"].iter().any(|e| n.ends_with(e)) || n.ends_with(".ld.in") || n.ends_with(".ld.s")
}

/// Flash partition tables (ESP-IDF, Nordic partition manager, Zephyr/MCUboot maps).
pub fn is_partition_table(path: &str) -> bool {
    let n = file_name(path).to_ascii_lowercase();
    (n.starts_with("partition") && n.ends_with(".csv")) || n == "pm_static.yml" || n.contains("flash_map") || n.contains("flash_layout")
}

/// Start-up code, vector tables and the CMSIS clock set-up file.
pub fn is_startup_file(path: &str) -> bool {
    let n = file_name(path).to_ascii_lowercase();
    let asm = [".s", ".asm"].iter().any(|e| n.ends_with(e));
    n.starts_with("startup_") || n.starts_with("crt0") || (n.contains("vector") && (asm || n.ends_with(".c"))) || (n.starts_with("system_") && n.ends_with(".c"))
}

pub fn is_third_party(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.split(['/', '\\']).any(|s| matches!(s, "third_party" | "third-party" | "thirdparty" | "vendor" | "external" | "extern" | "node_modules"))
}

#[derive(Debug)]
pub struct Func<'a> {
    pub name: &'a str,
    pub name_tok: usize,
    /// Token indices of the body's `{` and `}`.
    pub open: usize,
    pub close: usize,
    pub line: u32,
    pub end_line: u32,
    pub is_isr: bool,
}

const NOT_A_FUNCTION: [&str; 22] = [
    "if", "for", "while", "switch", "catch", "sizeof", "return", "do", "else", "defined", "alignas", "alignof", "decltype",
    "noexcept", "__attribute__", "__declspec", "_Static_assert", "static_assert", "typeof", "__typeof__", "requires", "throw",
];

fn is_isr_name(name: &str) -> bool {
    name.ends_with("_IRQHandler")
        || name.ends_with("_Handler") && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        || name.ends_with("_ISR")
        || name.ends_with("_isr")
        || name.ends_with("_irq")
        || name.starts_with("ISR_")
        || name.starts_with("isr_")
        || name == "ISR"
        || name == "SIGNAL"
}

pub struct FileCtx<'a> {
    pub path: &'a str,
    pub lang: Lang,
    pub text: &'a str,
    pub lx: Lexed<'a>,
    pub funcs: Vec<Func<'a>>,
    pub profile: &'a Profile,
}

impl<'a> FileCtx<'a> {
    pub fn new(path: &'a str, text: &'a str, profile: &'a Profile) -> Self {
        let mut lang = lang_of(path);
        if lang == Lang::C && profile.headers_cpp && path.to_ascii_lowercase().ends_with(".h") {
            lang = Lang::Cpp;
        }
        let lx =if matches!(lang, Lang::C | Lang::Cpp) { lex(text) } else { Lexed::default() };
        let mut cx = FileCtx { path, lang, text, lx, funcs: Vec::new(), profile };
        cx.find_functions();
        cx
    }

    pub fn toks(&self) -> &[Tok<'a>] {
        &self.lx.toks
    }
    pub fn comments(&self) -> &[Comment<'a>] {
        &self.lx.comments
    }
    pub fn is_c_family(&self) -> bool {
        matches!(self.lang, Lang::C | Lang::Cpp)
    }
    /// Production firmware source: not a test, not adopted code.
    pub fn is_production(&self) -> bool {
        !is_test_path(self.path) && !is_third_party(self.path)
    }
    pub fn pair(&self, i: usize) -> Option<usize> {
        match self.lx.pair.get(i) {
            Some(&p) if p != NONE => Some(p as usize),
            _ => None,
        }
    }
    /// Next code token (skipping preprocessor lines) after `i`.
    pub fn next(&self, i: usize) -> Option<usize> {
        (i + 1..self.lx.toks.len()).find(|&k| !self.lx.toks[k].pp)
    }
    pub fn prev(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&k| !self.lx.toks[k].pp)
    }
    pub fn func_at(&self, tok: usize) -> Option<&Func<'a>> {
        self.funcs.iter().find(|f| f.open <= tok && tok <= f.close)
    }
    /// `name(` used as a call: not a member access and not a declaration.
    pub fn is_call(&self, i: usize) -> bool {
        let t = &self.lx.toks;
        t[i].kind == Kind::Ident
            && self.next(i).is_some_and(|n| t[n].is("("))
            && !self.prev(i).is_some_and(|p| t[p].is(".") || t[p].is("->"))
    }

    fn find_functions(&mut self) {
        let t = &self.lx.toks;
        let mut i = 0;
        while i < t.len() {
            if t[i].pp || !t[i].is(")") {
                i += 1;
                continue;
            }
            let Some(open_paren) = self.pair(i) else {
                i += 1;
                continue;
            };
            // Skip trailing qualifiers between `)` and `{`.
            let mut j = i + 1;
            while j < t.len() && (t[j].pp || matches!(t[j].text, "const" | "noexcept" | "override" | "final" | "volatile")) {
                j += 1;
            }
            let name_tok = self.prev(open_paren);
            let ok = j < t.len()
                && t[j].is("{")
                && name_tok.is_some_and(|k| t[k].kind == Kind::Ident && !NOT_A_FUNCTION.contains(&t[k].text));
            if !ok {
                i += 1;
                continue;
            }
            let (name_tok, open) = (name_tok.unwrap(), j);
            let Some(close) = self.pair(open) else {
                i += 1;
                continue;
            };
            // Attributes sit between the previous statement and the name.
            let mut attr = false;
            let mut k = name_tok;
            while let Some(p) = self.prev(k) {
                if t[p].is(";") || t[p].is("}") || t[p].is("{") {
                    break;
                }
                if matches!(t[p].text, "__interrupt" | "interrupt" | "IRAM_ATTR" | "__irq" | "ISR_ATTR") {
                    attr = true;
                }
                k = p;
            }
            let name = t[name_tok].text;
            self.funcs.push(Func {
                name,
                name_tok,
                open,
                close,
                line: t[name_tok].line,
                end_line: t[close].line,
                is_isr: attr || is_isr_name(name),
            });
            i = close + 1;
        }
    }
}

/// A deviation marker found in a comment.
#[derive(Debug, PartialEq, Eq)]
pub struct Deviation<'a> {
    pub rule_id: &'a str,
    pub reason: &'a str,
    pub line: u32,
    pub end_line: u32,
}

pub const DEVIATION_MARK: &str = "EMB-DEVIATION(";

/// Deviation markers in `text`, whatever the comment syntax of the file.
pub fn deviations(text: &str) -> Vec<Deviation<'_>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find(DEVIATION_MARK) {
            let after = &rest[at + DEVIATION_MARK.len()..];
            let Some(end) = after.find(')') else { break };
            let reason = after[end + 1..].trim_start_matches([':', ' ', '\t']).trim_end_matches("*/").trim();
            out.push(Deviation { rule_id: after[..end].trim(), reason, line: n as u32 + 1, end_line: n as u32 + 1 });
            rest = &after[end + 1..];
        }
    }
    out
}

/// The shortest justification a deviation may carry.
pub const MIN_REASON: usize = 20;
