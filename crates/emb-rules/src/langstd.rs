//! Language editions: constructs newer than the C or C++ edition in
//! `[language]`, the C11 features MISRA C:2012 Rule 1.4 still restricts, and
//! build files that select a different edition from the profile.
//!
//! Nool 8.0.2's C and C++ adapters do not model the standard edition, and its
//! syntax check drops `-std` from `compile_commands.json` (bug 13aed9c2), so
//! the profile is the single place the edition is fixed.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{FileCtx, Rule, file_name};
use crate::profile::{c_name, cpp_name, parse_c_standard, parse_cpp_standard};

/// A C construct and the edition (year) that introduced it.
const C_WORDS: [(&str, u16); 21] = [
    ("inline", 1999),
    ("restrict", 1999),
    ("_Bool", 1999),
    ("_Complex", 1999),
    ("_Imaginary", 1999),
    ("_Generic", 2011),
    ("_Noreturn", 2011),
    ("_Static_assert", 2011),
    ("_Alignas", 2011),
    ("_Alignof", 2011),
    ("_Atomic", 2011),
    ("_Thread_local", 2011),
    ("nullptr", 2023),
    ("constexpr", 2023),
    ("typeof", 2023),
    ("typeof_unqual", 2023),
    ("_BitInt", 2023),
    ("_Decimal32", 2023),
    ("_Decimal64", 2023),
    ("_Decimal128", 2023),
    ("nullptr_t", 2023),
];
const C_HEADERS: [(&str, u16); 13] = [
    ("stdbool.h", 1999),
    ("stdint.h", 1999),
    ("inttypes.h", 1999),
    ("complex.h", 1999),
    ("tgmath.h", 1999),
    ("fenv.h", 1999),
    ("stdalign.h", 2011),
    ("stdatomic.h", 2011),
    ("stdnoreturn.h", 2011),
    ("threads.h", 2011),
    ("uchar.h", 2011),
    ("stdbit.h", 2023),
    ("stdckdint.h", 2023),
];
/// Directives that both C23 and C++23 added.
const DIRECTIVES_23: [&str; 4] = ["embed", "elifdef", "elifndef", "warning"];

const CPP_WORDS: [(&str, u16); 9] = [
    ("make_unique", 2014),
    ("co_await", 2020),
    ("co_yield", 2020),
    ("co_return", 2020),
    ("consteval", 2020),
    ("constinit", 2020),
    ("char8_t", 2020),
    ("concept", 2020),
    ("requires", 2020),
];
const CPP_HEADERS: [(&str, u16); 32] = [
    ("shared_mutex", 2014),
    ("optional", 2017),
    ("variant", 2017),
    ("any", 2017),
    ("string_view", 2017),
    ("filesystem", 2017),
    ("charconv", 2017),
    ("execution", 2017),
    ("memory_resource", 2017),
    ("concepts", 2020),
    ("coroutine", 2020),
    ("ranges", 2020),
    ("span", 2020),
    ("format", 2020),
    ("bit", 2020),
    ("numbers", 2020),
    ("source_location", 2020),
    ("compare", 2020),
    ("version", 2020),
    ("barrier", 2020),
    ("latch", 2020),
    ("semaphore", 2020),
    ("stop_token", 2020),
    ("syncstream", 2020),
    ("expected", 2023),
    ("print", 2023),
    ("stacktrace", 2023),
    ("mdspan", 2023),
    ("flat_map", 2023),
    ("flat_set", 2023),
    ("generator", 2023),
    ("stdfloat", 2023),
];
const CPP_ATTRS: [(&str, u16); 8] = [
    ("deprecated", 2014),
    ("nodiscard", 2017),
    ("fallthrough", 2017),
    ("maybe_unused", 2017),
    ("likely", 2020),
    ("unlikely", 2020),
    ("no_unique_address", 2020),
    ("assume", 2023),
];

/// C11 features MISRA C:2012 Rule 1.4 restricts after Amendment 3 removed
/// `_Generic`, `_Noreturn`/`<stdnoreturn.h>` and `_Alignas`/`<stdalign.h>`.
const EMERGENT_WORDS: [&str; 2] = ["_Atomic", "_Thread_local"];
const EMERGENT_HEADERS: [&str; 2] = ["stdatomic.h", "threads.h"];
/// Annex K bounds-checking interfaces.
const ANNEX_K: [&str; 24] = [
    "memcpy_s", "memmove_s", "memset_s", "strcpy_s", "strncpy_s", "strcat_s", "strncat_s", "strtok_s", "strerror_s", "strnlen_s",
    "sprintf_s", "snprintf_s", "vsprintf_s", "vsnprintf_s", "printf_s", "fprintf_s", "sscanf_s", "scanf_s", "fopen_s", "gets_s",
    "getenv_s", "qsort_s", "bsearch_s", "set_constraint_handler_s",
];

fn year_of<const N: usize>(table: &[(&str, u16); N], w: &str) -> Option<u16> {
    table.iter().find(|(k, _)| *k == w).map(|(_, y)| *y)
}

/// Constructs in a C file newer than the profile's C edition.
pub fn c_constructs(s: &mut Sink, rule: &'static Rule) {
    let cx = s.cx;
    let Some(target) = cx.profile.c_standard else { return };
    let too_new = |y: u16| y > target;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        match tok.kind {
            Kind::Ident => {
                if let Some(y) = year_of(&C_WORDS, tok.text).filter(|y| too_new(*y)) {
                    s.hit(rule, tok.line, format!("`{}` is {}; this target is built as {}", tok.text, c_name(y), c_name(target)));
                }
            }
            Kind::Number if too_new(2023) && is_binary_literal(tok.text) => {
                s.hit(rule, tok.line, format!("binary literal `{}` is C23; this target is built as {}", tok.text, c_name(target)));
            }
            Kind::Punct if tok.text == "[" && !tok.pp && too_new(2023) && t.get(i + 1).is_some_and(|n| n.is("[")) => {
                s.hit(rule, tok.line, format!("`[[...]]` attribute syntax is C23; this target is built as {}", c_name(target)));
            }
            Kind::Hash => directive_and_header(s, rule, i, target, &C_HEADERS, c_name),
            _ => {}
        }
    }
    if target < 1999 {
        for c in cx.comments().iter().filter(|c| !c.block) {
            s.hit(rule, c.line, "`//` comments are C99; this target is built as C90".to_string());
        }
    }
}

/// Constructs in a C++ file newer than the profile's C++ edition.
pub fn cpp_constructs(s: &mut Sink, rule: &'static Rule) {
    let cx = s.cx;
    let Some(target) = cx.profile.cpp_standard else { return };
    let t = cx.toks();
    let hit = |s: &mut Sink, line: u32, what: String, y: u16| {
        if y > target {
            s.hit(rule, line, format!("{what} is {}; this target is built as {}", cpp_name(y), cpp_name(target)));
        }
    };
    for (i, tok) in t.iter().enumerate() {
        let next = |k: usize| cx.next(i).and_then(|n| if k == 1 { Some(n) } else { cx.next(n) }).map(|n| &t[n]);
        match tok.kind {
            Kind::Ident if tok.pp => {}
            Kind::Ident => {
                let w = tok.text;
                if w == "if" && next(1).is_some_and(|n| n.is("constexpr")) {
                    hit(s, tok.line, "`if constexpr`".into(), 2017);
                } else if w == "if" && (next(1).is_some_and(|n| n.is("consteval")) || next(1).is_some_and(|n| n.is("!")) && next(2).is_some_and(|n| n.is("consteval"))) {
                    hit(s, tok.line, "`if consteval`".into(), 2023);
                } else if w == "decltype" && next(1).is_some_and(|n| n.is("(")) && next(2).is_some_and(|n| n.is("auto")) {
                    hit(s, tok.line, "`decltype(auto)`".into(), 2014);
                } else if w == "auto" && next(1).is_some_and(|n| n.is("[") || ((n.is("&") || n.is("&&")) && next(2).is_some_and(|m| m.is("[")))) {
                    hit(s, tok.line, "a structured binding (`auto [...]`)".into(), 2017);
                } else if w == "namespace" && next(1).is_some_and(|n| n.kind == Kind::Ident) && next(2).is_some_and(|n| n.is("::")) {
                    hit(s, tok.line, "a nested namespace definition (`namespace a::b`)".into(), 2017);
                } else if w == "consteval" && cx.prev(i).is_some_and(|p| t[p].is("if") || t[p].is("!")) {
                    // Counted as `if consteval` above.
                } else if w == "export" && next(1).is_some_and(|n| n.is("module")) {
                    hit(s, tok.line, "a module declaration".into(), 2020);
                } else if let Some(y) = year_of(&CPP_WORDS, w) {
                    hit(s, tok.line, format!("`{w}`"), y);
                }
            }
            Kind::Number if is_binary_literal(tok.text) => hit(s, tok.line, format!("binary literal `{}`", tok.text), 2014),
            Kind::Punct if tok.text == "<=>" => hit(s, tok.line, "the three-way comparison `<=>`".into(), 2020),
            Kind::Punct if tok.text == "[" && !tok.pp && t.get(i + 1).is_some_and(|n| n.is("[")) => {
                // `[[attr]]` or `[[ns::attr(...)]]`: the first name decides.
                if let Some(a) = t.get(i + 2).and_then(|a| a.ident())
                    && let Some(y) = year_of(&CPP_ATTRS, a)
                {
                    hit(s, tok.line, format!("`[[{a}]]`"), y);
                }
            }
            Kind::Hash => directive_and_header(s, rule, i, target, &CPP_HEADERS, cpp_name),
            _ => {}
        }
    }
}

fn is_binary_literal(n: &str) -> bool {
    let l = n.as_bytes();
    l.len() > 2 && l[0] == b'0' && (l[1] == b'b' || l[1] == b'B')
}

fn directive_and_header<const N: usize>(s: &mut Sink, rule: &'static Rule, i: usize, target: u16, headers: &[(&str, u16); N], name: fn(u16) -> &'static str) {
    let cx = s.cx;
    let t = cx.toks();
    let Some(dir) = t.get(i + 1).filter(|d| d.pp && d.line == t[i].line) else { return };
    if DIRECTIVES_23.contains(&dir.text) && target < 2023 {
        s.hit(rule, t[i].line, format!("`#{}` is {}; this target is built as {}", dir.text, name(2023), name(target)));
    }
    if dir.text == "include"
        && let Some(h) = t.get(i + 2).and_then(|h| h.str_body())
        && let Some(y) = year_of(headers, h).filter(|y| *y > target)
    {
        s.hit(rule, t[i].line, format!("<{h}> is {}; this target is built as {}", name(y), name(target)));
    }
}

/// MISRA C:2012 Rule 1.4 as amended by AMD3: `_Atomic`, `<stdatomic.h>`,
/// `_Thread_local`, `<threads.h>` and Annex K. Only on C11 or newer targets
/// (or an unset edition); an older target already refuses them as too new.
pub fn emergent(s: &mut Sink, rule: &'static Rule) {
    let cx = s.cx;
    if cx.profile.c_standard.is_some_and(|y| y < 2011) {
        return;
    }
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        match tok.kind {
            Kind::Ident if !tok.pp && EMERGENT_WORDS.contains(&tok.text) => {
                s.hit(rule, tok.line, format!("`{}`", tok.text));
            }
            Kind::Ident if !tok.pp && ANNEX_K.contains(&tok.text) && cx.is_call(i) => {
                s.hit(rule, tok.line, format!("{}() is an Annex K bounds-checking interface", tok.text));
            }
            Kind::Hash => {
                let Some(dir) = t.get(i + 1).filter(|d| d.pp && d.line == tok.line) else { continue };
                if dir.text == "include"
                    && let Some(h) = t.get(i + 2).and_then(|h| h.str_body()).filter(|h| EMERGENT_HEADERS.contains(h))
                {
                    s.hit(rule, tok.line, format!("#include <{h}>"));
                }
                if dir.text == "define"
                    && t.get(i + 2).is_some_and(|n| n.is("__STDC_WANT_LIB_EXT1__"))
                    && !t.get(i + 3).is_some_and(|v| v.line == tok.line && v.text == "0")
                {
                    s.hit(rule, tok.line, "__STDC_WANT_LIB_EXT1__ enables Annex K; only defining it to 0 is allowed".to_string());
                }
            }
            _ => {}
        }
    }
}

/// Whether a build or tool file can select a language edition.
pub fn is_build_selector(cx: &FileCtx) -> bool {
    cx.lang == crate::model::Lang::Build || file_name(cx.path).eq_ignore_ascii_case("compile_commands.json")
}

/// Editions a build file selects, as `(line, is_cpp, year, spelling)`.
pub fn selected_editions(text: &str) -> Vec<(u32, bool, u16, String)> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.split('#').next().unwrap_or("");
        let line_no = n as u32 + 1;
        let mut rest = code;
        while let Some(at) = rest.find("-std=") {
            let v: String = rest[at + 5..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '+').collect();
            push_edition(&mut out, line_no, &v, &format!("-std={v}"));
            rest = &rest[at + 5..];
        }
        let words: Vec<&str> = code.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | '\'' | '"' | '[' | ']' | '=')).filter(|w| !w.is_empty()).collect();
        for (k, w) in words.iter().enumerate() {
            let val = words.get(k + 1).copied().unwrap_or("");
            match *w {
                "CMAKE_C_STANDARD" | "C_STANDARD" => push_typed(&mut out, line_no, false, val, &format!("{w} {val}")),
                "CMAKE_CXX_STANDARD" | "CXX_STANDARD" => push_typed(&mut out, line_no, true, val, &format!("{w} {val}")),
                "c_std" => push_typed(&mut out, line_no, false, val, &format!("c_std={val}")),
                "cpp_std" => push_typed(&mut out, line_no, true, val, &format!("cpp_std={val}")),
                _ => {
                    if let Some(v) = w.strip_prefix("cxx_std_") {
                        push_typed(&mut out, line_no, true, v, w);
                    } else if let Some(v) = w.strip_prefix("c_std_") {
                        push_typed(&mut out, line_no, false, v, w);
                    }
                }
            }
        }
    }
    out
}

fn push_edition(out: &mut Vec<(u32, bool, u16, String)>, line: u32, v: &str, spelled: &str) {
    let l = v.to_ascii_lowercase();
    if l.contains("++") {
        if let Some(y) = parse_cpp_standard(&l) {
            out.push((line, true, y, spelled.to_string()));
        }
    } else if let Some(y) = parse_c_standard(&l) {
        out.push((line, false, y, spelled.to_string()));
    }
}

fn push_typed(out: &mut Vec<(u32, bool, u16, String)>, line: u32, cpp: bool, v: &str, spelled: &str) {
    let y = if cpp { parse_cpp_standard(v) } else { parse_c_standard(v) };
    if let Some(y) = y {
        out.push((line, cpp, y, spelled.to_string()));
    }
}

/// A build file that selects a different edition from the profile.
pub fn build_editions(s: &mut Sink, cpp: bool, rule: &'static Rule) {
    let cx = s.cx;
    let target = if cpp { cx.profile.cpp_standard } else { cx.profile.c_standard };
    let Some(target) = target else { return };
    let name = if cpp { cpp_name } else { c_name };
    let mut last = 0u32;
    for (line, is_cpp, y, spelled) in selected_editions(cx.text) {
        if is_cpp == cpp && y != target && line != last {
            last = line;
            s.hit(rule, line, format!("`{spelled}` builds as {}; [language] says {}", name(y), name(target)));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn prof(lang: &str) -> Profile {
        Profile::parse(&format!("[language]\n{lang}\n")).unwrap()
    }
    fn ids(pack: Pack, p: &Profile, path: &str, src: &str) -> Vec<&'static str> {
        check(pack, &[(path, src)], p).iter().map(|f| f.rule.id).collect()
    }
    fn c(p: &Profile, src: &str) -> Vec<&'static str> {
        ids(Pack::CSubset, p, "src/a.c", src)
    }
    fn cpp(p: &Profile, src: &str) -> Vec<&'static str> {
        ids(Pack::CppSubset, p, "src/a.cpp", src)
    }

    #[test]
    fn c020_constructs_newer_than_the_c_edition() {
        let c99 = prof("c_standard = \"c99\"");
        let f = check(Pack::CSubset, &[("src/a.c", "_Static_assert(1, \"x\");\n")], &c99);
        assert_eq!(f.len(), 1);
        let (msg, ev) = render(&f, &c99);
        assert!(msg.contains("EMB-C-020") && msg.contains("is C11; this target is built as C99"), "{msg}");
        assert!(ev.contains("MISRA C:2012 AMD2 1.2"));
        assert_eq!(c(&c99, "#include <stdalign.h>\n"), ["EMB-C-020"]);
        assert_eq!(c(&c99, "int f(void) { return _Generic(1, int: 1, default: 0); }\n"), ["EMB-C-020"]);
        // C23 over C11/C17.
        let c11 = prof("c_standard = \"gnu11\"");
        for src in ["int *p = nullptr;\n", "constexpr int N = 4;\n", "typeof(1) x;\n", "int m = 0b1010;\n", "[[nodiscard]] int f(void);\n", "#elifdef X\n", "#include <stdckdint.h>\n"] {
            assert_eq!(c(&c11, src), ["EMB-C-020"], "{src}");
        }
        assert_eq!(c(&prof("c_standard = \"c17\""), "int *p = nullptr;\n"), ["EMB-C-020"]);
        // C99 over C90, including // comments.
        let c90 = prof("c_standard = \"c90\"");
        assert_eq!(c(&c90, "#include <stdint.h>\n"), ["EMB-C-020"]);
        assert_eq!(c(&c90, "static inline int f(void) { return 1; }\n"), ["EMB-C-020"]);
        assert_eq!(c(&c90, "int x; // note\n"), ["EMB-C-020"]);
        // Within the edition, and with no edition set: nothing.
        assert!(c(&c11, "_Static_assert(1, \"x\");\n#include <stdint.h>\nstatic inline int f(void) { return 1; }\n").is_empty());
        assert!(c(&prof("c_standard = \"c23\""), "int *p = nullptr;\nint m = 0b1;\n").is_empty());
        assert!(c(&Profile::default(), "int *p = nullptr;\n").is_empty());
    }
    #[test]
    fn c021_emergent_features_after_amd3() {
        let c11 = prof("c_standard = \"c11\"");
        for src in [
            "_Atomic int counter;\n",
            "#include <stdatomic.h>\n",
            "_Thread_local int t;\n",
            "#include <threads.h>\n",
            "void f(char *d, const char *s) { strcpy_s(d, 8, s); }\n",
            "#define __STDC_WANT_LIB_EXT1__ 1\n",
        ] {
            assert_eq!(c(&c11, src), ["EMB-C-021"], "{src}");
        }
        let f = check(Pack::CSubset, &[("src/a.c", "_Atomic int c;\n")], &c11);
        assert!(render(&f, &c11).1.contains("MISRA C:2012 AMD2 Rule 1.4; MISRA C:2012 AMD3 2.3.1"));
        // AMD3 lifted the restriction on these.
        assert!(c(&c11, "#include <stdalign.h>\n#include <stdnoreturn.h>\n_Noreturn void halt(void);\n_Alignas(8) char b[8];\nint g(void) { return _Generic(1, int: 1, default: 0); }\n").is_empty());
        assert!(c(&c11, "#define __STDC_WANT_LIB_EXT1__ 0\n").is_empty());
        // No edition set: still C11 code, still restricted.
        assert_eq!(c(&Profile::default(), "_Atomic int c;\n"), ["EMB-C-021"]);
        // A C99 target reports it once, as too new.
        assert_eq!(c(&prof("c_standard = \"c99\""), "_Atomic int c;\n"), ["EMB-C-020"]);
    }
    #[test]
    fn std_build_files_select_the_profile_edition() {
        let p = prof("c_standard = \"c99\"\ncpp_standard = \"c++14\"");
        let cases: [(&str, &str, &str); 7] = [
            ("CMakeLists.txt", "set(CMAKE_C_STANDARD 11)\n", "EMB-C-022"),
            ("CMakeLists.txt", "set_target_properties(fw PROPERTIES C_STANDARD 11)\n", "EMB-C-022"),
            ("Makefile", "CFLAGS += -std=gnu11 -Wall\n", "EMB-C-022"),
            ("meson.build", "project('fw', 'c', default_options : ['c_std=c11'])\n", "EMB-C-022"),
            ("CMakeLists.txt", "set(CMAKE_CXX_STANDARD 17)\n", "EMB-CPP-020"),
            ("CMakeLists.txt", "target_compile_features(fw PUBLIC cxx_std_20)\n", "EMB-CPP-020"),
            (
                "build/compile_commands.json",
                "[{\"directory\":\"/w\",\"file\":\"/w/src/a.c\",\"arguments\":[\"cc\",\"-std=c11\",\"-c\",\"src/a.c\"]}]\n",
                "EMB-C-022",
            ),
        ];
        for (path, src, want) in cases {
            let mut got = ids(Pack::CSubset, &p, path, src);
            got.extend(ids(Pack::CppSubset, &p, path, src));
            assert_eq!(got, [want], "{path}: {src}");
        }
        let f = check(Pack::CppSubset, &[("CMakeLists.txt", "set(CMAKE_CXX_STANDARD 17)\n")], &p);
        assert!(render(&f, &p).0.contains("`CMAKE_CXX_STANDARD 17` builds as C++17; [language] says C++14"));
        // The command-string form of compile_commands.json.
        let cmd = "[{\"directory\":\"/w\",\"file\":\"src/x.cpp\",\"command\":\"arm-none-eabi-g++ -std=gnu++17 -c src/x.cpp\"}]\n";
        assert_eq!(ids(Pack::CppSubset, &p, "compile_commands.json", cmd), ["EMB-CPP-020"]);
        // Matching editions, comments, third-party and unset profile: nothing.
        let ok = "set(CMAKE_C_STANDARD 99)\nset(CMAKE_CXX_STANDARD 14)\nadd_compile_options(-std=gnu99)\n# set(CMAKE_C_STANDARD 11)\n";
        assert!(ids(Pack::CSubset, &p, "CMakeLists.txt", ok).is_empty() && ids(Pack::CppSubset, &p, "CMakeLists.txt", ok).is_empty());
        assert!(ids(Pack::CSubset, &p, "third_party/lib/CMakeLists.txt", "set(CMAKE_C_STANDARD 11)\n").is_empty());
        assert!(ids(Pack::CSubset, &Profile::default(), "Makefile", "CFLAGS = -std=c11\n").is_empty());
    }
    #[test]
    fn cpp019_constructs_newer_than_the_cpp_edition() {
        let p14 = prof("cpp_standard = \"c++14\"");
        let f = check(Pack::CppSubset, &[("src/a.cpp", "int f() { if constexpr (true) { return 1; } return 0; }\n")], &p14);
        let (msg, ev) = render(&f, &p14);
        assert!(msg.contains("EMB-CPP-019") && msg.contains("`if constexpr` is C++17; this target is built as C++14"), "{msg}");
        assert!(ev.contains("AUTOSAR C++14 A1-1-1"));
        for src in [
            "#include <optional>\n",
            "auto [a, b] = pair;\n",
            "const auto& [k, v] = *it;\n",
            "namespace fw::drv { }\n",
            "[[nodiscard]] int f();\n",
            "template <typename T> concept Small = sizeof(T) < 4;\n",
            "auto operator<=>(const P&) const = default;\n",
            "#include <span>\n",
        ] {
            assert_eq!(cpp(&p14, src), ["EMB-CPP-019"], "{src}");
        }
        let p11 = prof("cpp_standard = \"c++11\"");
        for src in ["decltype(auto) g();\n", "int m = 0b101;\n", "auto u = std::make_unique<int>(1);\n", "[[deprecated]] void old();\n"] {
            assert_eq!(cpp(&p11, src), ["EMB-CPP-019"], "{src}");
        }
        let p20 = prof("cpp_standard = \"c++20\"");
        for src in ["#include <expected>\n", "consteval int f() { if consteval { return 1; } return 0; }\n", "[[assume(x > 0)]];\n"] {
            assert_eq!(cpp(&p20, src), ["EMB-CPP-019"], "{src}");
        }
        assert!(cpp(&p20, "consteval int sq(int x) { return x * x; }\n#include <span>\n").is_empty());
        assert!(cpp(&Profile::default(), "#include <expected>\n").is_empty());
    }
    #[test]
    fn std_ignores_comments_strings_and_lookalike_names() {
        let c99 = prof("c_standard = \"c99\"");
        assert!(c(&c99, "/* nullptr _Atomic constexpr */\nconst char *s = \"_Static_assert nullptr\";\nint my_nullptr_count = 0;\n").is_empty());
        let p14 = prof("cpp_standard = \"c++14\"");
        assert!(cpp(&p14, "// if constexpr, #include <optional>\nconst char *s = \"co_await <=>\";\nint requires_count = 0;\nint arr[2]; int x = arr[ 1 ];\n").is_empty());
    }
    #[test]
    fn headers_cpp_makes_dot_h_files_cpp() {
        let src = "class Uart { public: void send(); };\n#include <optional>\n";
        let as_c = Profile::default();
        let as_cpp = prof("headers = \"cpp\"\ncpp_standard = \"c++14\"");
        assert!(ids(Pack::CppSubset, &as_c, "include/uart.h", src).is_empty());
        assert_eq!(ids(Pack::CppSubset, &as_cpp, "include/uart.h", src), ["EMB-CPP-019"]);
        // And the C pack no longer reads it as C.
        assert!(ids(Pack::CSubset, &prof("headers = \"cpp\"\nc_standard = \"c90\""), "include/uart.h", "int x; // c++ comment\n").is_empty());
        assert!(Profile::parse("[language]\nheaders = \"objc\"\n").is_err());
    }
    #[test]
    fn malformed_std_input_never_panics() {
        let p = prof("c_standard = \"c90\"\ncpp_standard = \"c++11\"");
        for src in ["[[", "#", "#include", "if", "auto", "-std=", "-std=c++", "CMAKE_C_STANDARD", "decltype(", "namespace", "0b", "export"] {
            for path in ["a.c", "a.cpp", "CMakeLists.txt", "compile_commands.json"] {
                let _ = check(Pack::CSubset, &[(path, src)], &p);
                let _ = check(Pack::CppSubset, &[(path, src)], &p);
            }
        }
    }
}
