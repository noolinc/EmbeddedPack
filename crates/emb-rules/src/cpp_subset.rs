//! `emb_cpp_subset`: the C++ language subset for production firmware.
//!
//! Every clause was checked against the AUTOSAR C++14 guidelines (R17-10) and
//! the JSF AV C++ coding standards. `M` rules are the MISRA C++:2008 rules
//! AUTOSAR adopts unchanged.

use crate::c_subset::{control_bodies, is_float_literal, switches};
use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, Lang, Pack, Rule, file_name};

macro_rules! rules {
    ($( $name:ident = $id:literal, $strict:literal, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::CppSubset, category: Category::Required, strict: $strict, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    C_CAST = "EMB-CPP-001", false, "traditional C-style casts shall not be used",
        "AUTOSAR C++14 A5-2-2; JSF AV Rule 185";
    REINTERPRET = "EMB-CPP-002", false, "reinterpret_cast shall not be used",
        "AUTOSAR C++14 A5-2-4";
    CONST_CAST = "EMB-CPP-003", false, "a cast shall not remove const or volatile",
        "AUTOSAR C++14 A5-2-3";
    NULL_MACRO = "EMB-CPP-004", false, "only nullptr shall be used as the null pointer constant",
        "AUTOSAR C++14 A4-10-1";
    GOTO = "EMB-CPP-005", true, "goto shall not be used",
        "AUTOSAR C++14 A6-6-1; JSF AV Rule 189";
    HEADERS = "EMB-CPP-006", false, "<cstdio>, <csetjmp>, <csignal>, <ctime> and <clocale> shall not be used",
        "AUTOSAR C++14 M27-0-1, M17-0-5, M18-7-1, M18-0-4, A18-0-3; JSF AV Rules 19-22, 25";
    FUNCTIONS = "EMB-CPP-007", false, "atoi/atof/atol, abort/exit/getenv/system, rand and setjmp/longjmp shall not be used",
        "AUTOSAR C++14 A18-0-2, M18-0-3, A26-5-1, M17-0-5; JSF AV Rules 20, 23, 24";
    VARIADIC = "EMB-CPP-008", false, "functions shall not be defined with the ellipsis notation",
        "AUTOSAR C++14 A8-4-1; JSF AV Rule 108";
    CONSTRUCTS = "EMB-CPP-009", false, "auto_ptr, vector<bool>, std::bind, register, long double, asm and union shall not be used",
        "AUTOSAR C++14 A18-1-3, A18-1-2, A18-9-1, A7-1-4, A0-4-2, A7-4-1, A9-5-1; JSF AV Rule 153";
    USING_IN_HEADER = "EMB-CPP-010", false, "using-directives shall not be used in header files",
        "AUTOSAR C++14 M7-3-6";
    UNSCOPED_ENUM = "EMB-CPP-011", false, "enumerations shall be scoped enum classes",
        "AUTOSAR C++14 A7-2-3";
    SWITCH_DEFAULT = "EMB-CPP-012", false, "every switch shall end in a default clause",
        "AUTOSAR C++14 M6-4-6; JSF AV Rule 194";
    FALLTHROUGH = "EMB-CPP-013", false, "a break or throw shall end every non-empty switch clause",
        "AUTOSAR C++14 M6-4-5; JSF AV Rule 193";
    BRACES = "EMB-CPP-014", false, "the body of if/else/for/while/do shall be a compound statement",
        "AUTOSAR C++14 M6-3-1, M6-4-1; JSF AV Rule 59";
    OCTAL = "EMB-CPP-015", false, "octal constants shall not be used",
        "AUTOSAR C++14 M2-13-2; JSF AV Rule 149";
    ASSIGN_COND = "EMB-CPP-016", false, "assignment operators shall not be used in sub-expressions",
        "AUTOSAR C++14 M6-2-1";
    FLOAT_EQ = "EMB-CPP-017", false, "floating-point expressions shall not be tested for equality",
        "AUTOSAR C++14 M6-2-2; JSF AV Rule 202";
    FLOAT_COUNTER = "EMB-CPP-018", false, "a loop counter shall not have floating type",
        "AUTOSAR C++14 A6-5-2; JSF AV Rule 197";
}

const BANNED_HEADERS: [&str; 10] =
    ["cstdio", "stdio.h", "csetjmp", "setjmp.h", "csignal", "signal.h", "ctime", "time.h", "clocale", "locale.h"];
const BANNED_FUNCTIONS: [&str; 12] =
    ["atoi", "atof", "atol", "atoll", "abort", "exit", "getenv", "system", "rand", "srand", "setjmp", "longjmp"];

/// Words that can stand before `(type)` when it is a cast.
const BEFORE_CAST: [&str; 6] = ["return", "case", "else", "throw", "co_return", "co_yield"];
const TYPE_WORDS: [&str; 16] = [
    "char", "short", "int", "long", "float", "double", "signed", "unsigned", "bool", "wchar_t", "size_t", "char16_t",
    "char32_t", "const", "volatile", "struct",
];

fn is_type_word(w: &str) -> bool {
    TYPE_WORDS.contains(&w) || w.ends_with("_t")
}

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if cx.lang != Lang::Cpp || !cx.is_production() {
        return;
    }
    tokens(s);
    includes(s);
    switches(s, &SWITCH_DEFAULT, &FALLTHROUGH);
    control_bodies(s, &BRACES, &ASSIGN_COND, &FLOAT_COUNTER);
}

fn is_header(path: &str) -> bool {
    let n = file_name(path).to_ascii_lowercase();
    [".hpp", ".hh", ".hxx", ".h++", ".ipp", ".tpp"].iter().any(|e| n.ends_with(e))
}

fn tokens(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let header = is_header(cx.path);
    for (i, tok) in t.iter().enumerate() {
        let prev = cx.prev(i).map(|p| &t[p]);
        let next = cx.next(i).map(|n| &t[n]);
        match tok.kind {
            Kind::Number => {
                let b = tok.text.as_bytes();
                if b.len() > 1 && b[0] == b'0' && b[1].is_ascii_digit() && !is_float_literal(tok.text) {
                    s.hit(&OCTAL, tok.line, format!("octal constant {}", tok.text));
                }
            }
            Kind::Punct if tok.pp => {}
            Kind::Punct => match tok.text {
                "==" | "!=" => {
                    let float = |k: Option<&crate::lex::Tok>| k.is_some_and(|k| k.kind == Kind::Number && is_float_literal(k.text));
                    if float(prev) || float(next) {
                        s.hit(&FLOAT_EQ, tok.line, format!("'{}' against a floating-point constant", tok.text));
                    }
                }
                // `, ...)` or `(...)`: a C variadic, not a pack expansion.
                "..." if next.is_some_and(|n| n.is(")")) && prev.is_some_and(|p| p.is(",") || p.is("(")) => {
                    let catch_all = prev.is_some_and(|p| p.is("(")) && cx.prev(i).and_then(|p| cx.prev(p)).is_some_and(|c| t[c].is("catch"));
                    if !catch_all {
                        s.hit(&VARIADIC, tok.line, "function declared with '...'");
                    }
                }
                "(" => {
                    if let Some(what) = c_style_cast(cx, i) {
                        s.hit(&C_CAST, tok.line, format!("C-style cast ({what})"));
                    }
                }
                _ => {}
            },
            Kind::Ident if tok.pp => {}
            Kind::Ident => match tok.text {
                "goto" => s.hit(&GOTO, tok.line, "goto"),
                "reinterpret_cast" => s.hit(&REINTERPRET, tok.line, "reinterpret_cast"),
                "const_cast" => s.hit(&CONST_CAST, tok.line, "const_cast"),
                "NULL" => s.hit(&NULL_MACRO, tok.line, "NULL; use nullptr"),
                "auto_ptr" => s.hit(&CONSTRUCTS, tok.line, "std::auto_ptr (A18-1-3)"),
                "register" => s.hit(&CONSTRUCTS, tok.line, "register keyword (A7-1-4)"),
                "union" => s.hit(&CONSTRUCTS, tok.line, "union (A9-5-1; AV Rule 153)"),
                "asm" | "__asm__" | "__asm" => s.hit(&CONSTRUCTS, tok.line, "asm declaration (A7-4-1)"),
                "long" if next.is_some_and(|n| n.is("double")) => s.hit(&CONSTRUCTS, tok.line, "long double (A0-4-2)"),
                "vector" if next.is_some_and(|n| n.is("<")) && cx.next(i).and_then(|n| cx.next(n)).is_some_and(|b| t[b].is("bool")) => {
                    s.hit(&CONSTRUCTS, tok.line, "std::vector<bool> (A18-1-2)")
                }
                "bind" if prev.is_some_and(|p| p.is("::")) && cx.prev(i).and_then(|p| cx.prev(p)).is_some_and(|n| t[n].is("std")) => {
                    s.hit(&CONSTRUCTS, tok.line, "std::bind (A18-9-1)")
                }
                "using" if header && next.is_some_and(|n| n.is("namespace")) && cx.func_at(i).is_none() => {
                    s.hit(&USING_IN_HEADER, tok.line, "using namespace in a header")
                }
                "enum" if !next.is_some_and(|n| n.is("class") || n.is("struct")) => {
                    s.hit(&UNSCOPED_ENUM, tok.line, "unscoped enum; use enum class")
                }
                name if BANNED_FUNCTIONS.contains(&name) && cx.is_call(i) => {
                    // `std::exit(1)` is the same function; `obj::exit()` is not.
                    let qualified = prev.is_some_and(|p| p.is("::"));
                    let std_q = qualified && cx.prev(i).and_then(|p| cx.prev(p)).is_some_and(|n| t[n].is("std"));
                    if !qualified || std_q {
                        s.hit(&FUNCTIONS, tok.line, format!("call to {name}()"));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// `(type)expr`: a parenthesised type name followed by an operand.
fn c_style_cast(cx: &crate::model::FileCtx, open: usize) -> Option<String> {
    let t = cx.toks();
    let close = cx.pair(open)?;
    if close <= open + 1 {
        return None;
    }
    let inner = &t[open + 1..close];
    let mut words = 0;
    for (n, k) in inner.iter().enumerate() {
        match k.kind {
            Kind::Ident if is_type_word(k.text) => words += 1,
            // `std::uint8_t`: a namespace qualifier before the type name.
            Kind::Ident if inner.get(n + 1).is_some_and(|q| q.text == "::") => {}
            Kind::Punct if k.text == "::" => {}
            Kind::Punct if (k.text == "*" || k.text == "&") && n > 0 => {}
            _ => return None,
        }
    }
    if words == 0 {
        return None;
    }
    // What stands before: an operator or one of a few keywords, never a name
    // (that would be a call or a declaration) and never `)` or `]`.
    match cx.prev(open).map(|p| &t[p]) {
        Some(p) if p.kind == Kind::Ident && !BEFORE_CAST.contains(&p.text) => return None,
        Some(p) if p.kind == Kind::Number || p.kind == Kind::Str || p.is(")") || p.is("]") || p.is(">") => return None,
        _ => {}
    }
    // What follows: the operand.
    let n = &t[cx.next(close)?];
    let operand = matches!(n.kind, Kind::Ident | Kind::Number | Kind::Str | Kind::Char) || matches!(n.text, "(" | "*" | "&" | "-" | "!" | "~");
    if !operand || matches!(n.text, "const" | "noexcept" | "override" | "final") {
        return None;
    }
    Some(inner.iter().map(|k| k.text).collect::<Vec<_>>().join(" "))
}

fn includes(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.kind == Kind::Hash && t.get(i + 1).is_some_and(|d| d.is("include")) {
            if let Some(h) = t.get(i + 2).and_then(|h| h.str_body()).filter(|h| BANNED_HEADERS.contains(h)) {
                s.hit(&HEADERS, tok.line, format!("#include <{h}>"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{all_rules, check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn ids_at(path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::CppSubset, &[(path, src)], &Profile::default()).iter().map(|f| f.rule.id).collect()
    }
    fn ids(src: &str) -> Vec<&'static str> {
        ids_at("src/app.cpp", src)
    }
    fn rule(id: &str, clause: &str, bad: &str, good: &str) {
        let p = Profile::default();
        let f = check(Pack::CppSubset, &[("src/app.cpp", bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids(bad));
        let (_, ev) = render(&f, &p);
        assert!(ev.contains(id) && ev.contains(clause), "{ev}");
        assert!(!ids(good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn cpp001_c_style_cast() {
        rule(
            "EMB-CPP-001",
            "AUTOSAR C++14 A5-2-2; JSF AV Rule 185",
            "auto b = (uint8_t)x;",
            "auto b = static_cast<uint8_t>(x); void f(int); auto n = sizeof(int) * 2; (void)x; int g(uint8_t) const; auto h = f(int(3));",
        );
        assert!(ids("int f(float v) { return (int)v; }").contains(&"EMB-CPP-001"));
        assert!(ids("auto p = (const char *)buf;").contains(&"EMB-CPP-001"));
        assert!(ids("x = y + (unsigned long)(z);").contains(&"EMB-CPP-001"));
        assert!(ids("auto b = (std::uint8_t)v;").contains(&"EMB-CPP-001"));
        assert!(!ids("auto m = (std::max)(a, b); auto q = (ns::value) * 2;").contains(&"EMB-CPP-001"));
    }
    #[test]
    fn cpp002_reinterpret_cast() {
        rule("EMB-CPP-002", "AUTOSAR C++14 A5-2-4", "auto p = reinterpret_cast<uint32_t *>(addr);", "auto p = static_cast<uint32_t>(addr);");
    }
    #[test]
    fn cpp003_const_cast() {
        rule("EMB-CPP-003", "AUTOSAR C++14 A5-2-3", "auto p = const_cast<char *>(s);", "const char *p = s;");
    }
    #[test]
    fn cpp004_null() {
        rule("EMB-CPP-004", "AUTOSAR C++14 A4-10-1", "int *p = NULL;", "int *p = nullptr; int NULL_COUNT = 0;");
    }
    #[test]
    fn cpp005_goto() {
        rule("EMB-CPP-005", "AUTOSAR C++14 A6-6-1; JSF AV Rule 189", "void f() { goto out; out: return; }", "void f() { return; }");
    }
    #[test]
    fn cpp006_banned_headers() {
        rule("EMB-CPP-006", "M27-0-1", "#include <cstdio>\n", "#include <cstdint>\n#include <array>\n");
        for h in ["stdio.h", "csetjmp", "csignal", "ctime", "time.h", "clocale"] {
            assert!(ids(&format!("#include <{h}>\n")).contains(&"EMB-CPP-006"), "{h}");
        }
    }
    #[test]
    fn cpp007_banned_functions() {
        rule("EMB-CPP-007", "M18-0-3", "void f() { std::exit(1); }", "void f() { app.exit(1); Shell::exit(1); int rand = 3; }");
        assert!(ids("int f(const char *s) { return atoi(s); }").contains(&"EMB-CPP-007"));
        assert!(ids("int f() { return std::rand(); }").contains(&"EMB-CPP-007"));
    }
    #[test]
    fn cpp008_variadic_function() {
        rule(
            "EMB-CPP-008",
            "AUTOSAR C++14 A8-4-1; JSF AV Rule 108",
            "void log(const char *fmt, ...);",
            "template <typename... Args> void log(Args&&... args) { sink(std::forward<Args>(args)...); try { f(); } catch (...) { g(); } }",
        );
    }
    #[test]
    fn cpp009_banned_constructs() {
        rule("EMB-CPP-009", "A18-1-3", "std::auto_ptr<int> p;", "std::unique_ptr<int> p; std::vector<int> v; long x; double y;");
        for bad in ["std::vector<bool> flags;", "auto f = std::bind(g, 1);", "register int i;", "long double x;", "union U { int a; float b; };", "void f() { asm(\"nop\"); }"] {
            assert!(ids(bad).contains(&"EMB-CPP-009"), "{bad}");
        }
    }
    #[test]
    fn cpp010_using_namespace_in_header() {
        let bad = "using namespace std;\nclass A {};";
        assert!(ids_at("inc/a.hpp", bad).contains(&"EMB-CPP-010"));
        let p = Profile::default();
        let (_, ev) = render(&check(Pack::CppSubset, &[("inc/a.hpp", bad)], &p), &p);
        assert!(ev.contains("AUTOSAR C++14 M7-3-6"));
        assert!(!ids_at("src/a.cpp", bad).contains(&"EMB-CPP-010"));
        assert!(!ids_at("inc/a.hpp", "inline void f() { using namespace std; }").contains(&"EMB-CPP-010"));
    }
    #[test]
    fn cpp011_unscoped_enum() {
        rule("EMB-CPP-011", "AUTOSAR C++14 A7-2-3", "enum Color { Red, Green };", "enum class Color : uint8_t { Red, Green }; enum struct S { A };");
    }
    #[test]
    fn cpp012_switch_default() {
        rule(
            "EMB-CPP-012",
            "AUTOSAR C++14 M6-4-6; JSF AV Rule 194",
            "void f(int x) { switch (x) { case 1: g(); break; } }",
            "void f(int x) { switch (x) { case 1: g(); break; default: break; } }",
        );
    }
    #[test]
    fn cpp013_fallthrough() {
        rule(
            "EMB-CPP-013",
            "AUTOSAR C++14 M6-4-5; JSF AV Rule 193",
            "void f(int x) { switch (x) { case 1: g(); case 2: h(); break; default: break; } }",
            "void f(int x) { switch (x) { case 1: case 2: g(); break; case 3: throw Err{}; default: break; } }",
        );
    }
    #[test]
    fn cpp014_braces() {
        rule("EMB-CPP-014", "AUTOSAR C++14 M6-3-1, M6-4-1; JSF AV Rule 59", "void f() { if (a) g(); }", "void f() { if (a) { g(); } else { h(); } for (auto &x : xs) { g(x); } if constexpr (K) { g(); } }");
    }
    #[test]
    fn cpp015_octal() {
        rule("EMB-CPP-015", "AUTOSAR C++14 M2-13-2; JSF AV Rule 149", "int m = 0755;", "int m = 0; int h = 0x1F; int b = 0b101; auto n = 1'000;");
    }
    #[test]
    fn cpp016_assign_in_condition() {
        rule("EMB-CPP-016", "AUTOSAR C++14 M6-2-1", "void f() { if (a = b) { g(); } }", "void f() { if (a == b) { g(); } if (auto p = find()) { g(); } if (Foo *q = get()) { g(); } }");
    }
    #[test]
    fn cpp017_float_equality() {
        rule("EMB-CPP-017", "AUTOSAR C++14 M6-2-2; JSF AV Rule 202", "bool f(float x) { return x == 1.0F; }", "bool f(int x) { return x == 1; }");
    }
    #[test]
    fn cpp018_float_loop_counter() {
        rule("EMB-CPP-018", "AUTOSAR C++14 A6-5-2; JSF AV Rule 197", "void f() { for (double d = 0.0; d < 1.0; d += 0.1) { g(); } }", "void f() { for (std::size_t i = 0U; i < 3U; ++i) { g(); } }");
    }

    #[test]
    fn cpp_subset_ignores_comments_and_strings() {
        let src = "// goto x; (int)y; NULL\n/* union U {}; reinterpret_cast */\nconst char *s = \"goto NULL (int)x enum E\";\nauto r = R\"x(NULL goto \" union)x\";\n";
        assert_eq!(ids(src), Vec::<&str>::new());
    }
    #[test]
    fn cpp_subset_does_not_flag_valid_modern_cpp() {
        let src = r#"
#include <array>
#include <cstdint>

namespace drv {

enum class Mode : std::uint8_t { Idle, Run };

class Motor final {
public:
    Motor() = default;
    Motor(const Motor &) = delete;
    explicit Motor(std::uint32_t limit) noexcept : limit_(limit) {}

    template <typename... Args>
    void configure(Args &&... args) { apply(std::forward<Args>(args)...); }

    std::uint32_t step(Mode mode, std::uint32_t rpm) const
    {
        std::uint32_t out = 0U;
        if (auto capped = clamp(rpm)) {
            out = static_cast<std::uint32_t>(capped);
        }
        switch (mode) {
            case Mode::Idle:
                out = 0U;
                break;
            case Mode::Run: {
                out = rpm;
                break;
            }
            default:
                break;
        }
        (void)sizeof(std::uint32_t);
        return out * (limit_ + 1U);
    }

private:
    std::uint32_t clamp(std::uint32_t rpm) const { return (rpm > limit_) ? limit_ : rpm; }
    std::uint32_t limit_ = 0U;
    std::array<std::uint8_t, 4> buf_{};
    void (*hook_)(int) = nullptr;
};

}  // namespace drv
"#;
        assert_eq!(ids(src), Vec::<&str>::new());
    }
    #[test]
    fn cpp_subset_scope() {
        let bad = "int *p = NULL; void f() { goto x; x: return; }";
        assert!(!ids(bad).is_empty());
        for p in ["src/app.c", "src/app.h", "tests/app_test.cpp", "third_party/lib/x.cpp", "docs/x.md"] {
            assert_eq!(ids_at(p, bad), Vec::<&str>::new(), "{p}");
        }
        for p in ["src/a.cc", "src/a.cxx", "inc/a.hpp", "sketch/main.ino"] {
            assert!(!ids_at(p, bad).is_empty(), "{p}");
        }
    }
    #[test]
    fn no_unverified_misra_cpp_2023_citations() {
        for r in all_rules() {
            assert!(!r.refs.contains("MISRA C++:2023"), "{} cites paywalled, uncorroborated text", r.id);
        }
    }
    #[test]
    fn malformed_cpp_never_panics() {
        for src in ["(", "(int", "(int)", "enum", "std::", "vector<", "long", "using", "template <", "...)", "catch ("] {
            let _ = ids(src);
        }
    }
}
