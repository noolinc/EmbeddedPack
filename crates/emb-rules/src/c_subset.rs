//! `emb_c_subset`: the C language subset for production firmware.
//!
//! Clauses were checked against BARR-C:2018 and the free MISRA documents
//! (Amendment 2, Addendum 3). MISRA C:2012 itself is not redistributable.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, FileCtx, Lang, Pack, Rule};

macro_rules! rules {
    ($( $name:ident = $id:literal, $strict:literal, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::CSubset, category: Category::Required, strict: $strict, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    GOTO = "EMB-C-001", true, "goto shall not be used",
        "MISRA C:2012 Rule 15.1; BARR-C:2018 1.7c; Power of 10 Rule 1";
    SETJMP = "EMB-C-002", true, "setjmp/longjmp shall not be used",
        "MISRA C:2012 Rule 21.4; Power of 10 Rule 1";
    ATOX = "EMB-C-003", false, "atof, atoi, atol and atoll shall not be used",
        "MISRA C:2012 Rule 21.7";
    TERMINATION = "EMB-C-004", false, "abort, exit, _Exit, quick_exit and getenv shall not be used",
        "MISRA C:2012 Rule 21.8";
    SIGNAL = "EMB-C-005", false, "the facilities of <signal.h> shall not be used",
        "MISRA C:2012 Rule 21.5";
    STDARG = "EMB-C-006", false, "the features of <stdarg.h> shall not be used",
        "MISRA C:2012 Rule 17.1";
    STDIO = "EMB-C-007", false, "<stdio.h> input/output shall not be used in production code",
        "MISRA C:2012 Rule 21.6";
    OCTAL = "EMB-C-008", false, "octal constants shall not be used",
        "MISRA C:2012 Rule 7.1";
    NESTED_COMMENT = "EMB-C-009", false, "/* shall not appear inside a comment",
        "MISRA C:2012 Rule 3.1; BARR-C:2018 2.1b";
    RESERVED_MACRO = "EMB-C-010", false, "#define shall not be used on a reserved identifier",
        "MISRA C:2012 Rule 21.1; SEI CERT C DCL37-C";
    SWITCH_DEFAULT = "EMB-C-011", false, "every switch shall have a default",
        "MISRA C:2012 Rule 16.4; BARR-C:2018 8.3b";
    FALLTHROUGH = "EMB-C-012", false, "an unconditional break shall end every switch clause",
        "MISRA C:2012 Rule 16.3";
    BRACES = "EMB-C-013", false, "the body of if/else/for/while/do shall be a braced block",
        "MISRA C:2012 Rule 15.6; BARR-C:2018 1.3a";
    ASSIGN_COND = "EMB-C-014", false, "an assignment shall not be used as a condition",
        "MISRA C:2012 Rule 13.4; BARR-C:2018 8.2c; SEI CERT C EXP45-C";
    FLOAT_EQ = "EMB-C-015", false, "floating-point values shall not be tested for equality",
        "BARR-C:2018 5.4b.iv";
    FLOAT_COUNTER = "EMB-C-016", false, "a loop counter shall not have floating type",
        "MISRA C:2012 Rule 14.1; SEI CERT C FLP30-C";
    MACRO_PASTE = "EMB-C-017", false, "## and variadic macros shall not be used",
        "MISRA C:2012 Rule 20.10; Power of 10 Rule 8";
    REGISTER_AUTO = "EMB-C-018", false, "the register and auto keywords shall not be used",
        "BARR-C:2018 1.7a, 1.7b";
    BSEARCH_QSORT = "EMB-C-019", false, "bsearch and qsort shall not be used",
        "MISRA C:2012 Rule 21.9";
    EDITION = "EMB-C-020", false, "a C construct shall belong to the C edition the project is built for",
        "MISRA C:2012 AMD2 1.2; hardware profile: language.c_standard";
    EMERGENT = "EMB-C-021", false, "emergent language features shall not be used",
        "MISRA C:2012 AMD2 Rule 1.4; MISRA C:2012 AMD3 2.3.1";
    BUILD_EDITION = "EMB-C-022", false, "the build shall select the C edition in the hardware profile",
        "hardware profile: language.c_standard";
}

fn banned_call(name: &str) -> Option<&'static Rule> {
    Some(match name {
        "setjmp" | "longjmp" | "sigsetjmp" | "siglongjmp" => &SETJMP,
        "atof" | "atoi" | "atol" | "atoll" => &ATOX,
        "abort" | "exit" | "_Exit" | "quick_exit" | "getenv" => &TERMINATION,
        "signal" | "raise" => &SIGNAL,
        "va_start" | "va_arg" | "va_end" | "va_copy" => &STDARG,
        "bsearch" | "qsort" => &BSEARCH_QSORT,
        _ => return None,
    })
}

fn banned_header(name: &str) -> Option<&'static Rule> {
    Some(match name {
        "setjmp.h" => &SETJMP,
        "signal.h" => &SIGNAL,
        "stdarg.h" => &STDARG,
        "stdio.h" => &STDIO,
        _ => return None,
    })
}

pub(crate) fn is_float_literal(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    if l.starts_with("0x") {
        return l.contains('p');
    }
    l.contains('.') || l.contains('e') || l.ends_with('f')
}

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if cx.is_production() && crate::langstd::is_build_selector(cx) {
        crate::langstd::build_editions(s, false, &BUILD_EDITION);
        return;
    }
    if cx.lang != Lang::C || !cx.is_production() {
        return;
    }
    crate::langstd::c_constructs(s, &EDITION);
    crate::langstd::emergent(s, &EMERGENT);
    keywords_calls_literals(s);
    preprocessor(s);
    comments(s);
    switches(s, &SWITCH_DEFAULT, &FALLTHROUGH);
    control_bodies(s, &BRACES, &ASSIGN_COND, &FLOAT_COUNTER);
}

fn keywords_calls_literals(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        match tok.kind {
            Kind::Ident if !tok.pp => {
                match tok.text {
                    "goto" => s.hit(&GOTO, tok.line, "goto"),
                    "register" | "auto" => s.hit(&REGISTER_AUTO, tok.line, format!("'{}' keyword", tok.text)),
                    name => {
                        if let Some(rule) = banned_call(name).filter(|_| cx.is_call(i)) {
                            s.hit(rule, tok.line, format!("call to {name}()"));
                        }
                    }
                }
            }
            Kind::Number => {
                let n = tok.text;
                let b = n.as_bytes();
                if b.len() > 1 && b[0] == b'0' && b[1].is_ascii_digit() && !is_float_literal(n) {
                    s.hit(&OCTAL, tok.line, format!("octal constant {n}"));
                }
            }
            Kind::Punct if !tok.pp && (tok.text == "==" || tok.text == "!=") => {
                let float = |k: Option<usize>| k.is_some_and(|k| t[k].kind == Kind::Number && is_float_literal(t[k].text));
                if float(cx.prev(i)) || float(cx.next(i)) {
                    s.hit(&FLOAT_EQ, tok.line, format!("'{}' against a floating-point constant", tok.text));
                }
            }
            _ => {}
        }
    }
}

fn preprocessor(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.kind != Kind::Hash {
            continue;
        }
        let Some(dir) = t.get(i + 1).filter(|d| d.pp && d.line == tok.line) else { continue };
        // The directive runs to the next non-directive token.
        let end = (i + 2..t.len()).find(|&k| !t[k].pp || t[k].kind == Kind::Hash).unwrap_or(t.len());
        match dir.text {
            "include" => {
                if let Some(rule) = t.get(i + 2).and_then(|h| h.str_body()).and_then(banned_header) {
                    s.hit(rule, tok.line, format!("#include {}", t[i + 2].text));
                }
            }
            "define" | "undef" => {
                if let Some(name) = t.get(i + 2).and_then(|n| n.ident()) {
                    let b = name.as_bytes();
                    // The user is meant to define __STDC_WANT_LIB_EXT1__ (C11 K.3.1.1).
                    if b.len() > 1 && b[0] == b'_' && (b[1] == b'_' || b[1].is_ascii_uppercase()) && name != "__STDC_WANT_LIB_EXT1__" {
                        s.hit(&RESERVED_MACRO, tok.line, format!("#{} {name}", dir.text));
                    }
                }
                for k in i + 3..end {
                    if t[k].kind == Kind::Punct && (t[k].text == "##" || t[k].text == "...") {
                        s.hit(&MACRO_PASTE, tok.line, format!("'{}' in macro {}", t[k].text, t[i + 2].text));
                        break;
                    }
                }
            }
            _ => {}
        }
    }
}

fn comments(s: &mut Sink) {
    let cx = s.cx;
    for c in cx.comments() {
        if c.block && c.text.len() >= 4 && c.text[2..c.text.len() - 2].contains("/*") {
            s.hit(&NESTED_COMMENT, c.line, "'/*' inside a comment");
        }
    }
}

/// Switch shape, shared with the C++ subset under its own rule ids.
pub(crate) fn switches(s: &mut Sink, default_rule: &'static Rule, fall_rule: &'static Rule) {
    let cx = s.cx;
    let t = cx.toks();
    for i in 0..t.len() {
        if t[i].pp || !t[i].is("switch") || t[i].kind != Kind::Ident {
            continue;
        }
        let Some(body) = body_after_parens(cx, i) else { continue };
        let Some(end) = cx.pair(body) else { continue };
        // Labels directly in this switch: step over nested blocks.
        let mut labels: Vec<(usize, bool)> = Vec::new();
        let mut k = body + 1;
        while k < end {
            if t[k].pp {
                k += 1;
                continue;
            }
            if t[k].is("{") || t[k].is("(") || t[k].is("[") {
                k = cx.pair(k).map_or(k + 1, |p| p + 1);
                continue;
            }
            if t[k].kind == Kind::Ident && (t[k].text == "case" || t[k].text == "default") {
                labels.push((k, t[k].text == "default"));
            }
            k += 1;
        }
        if !labels.iter().any(|l| l.1) {
            s.hit(default_rule, t[i].line, "switch has no default");
        }
        for (n, &(label, _)) in labels.iter().enumerate() {
            let stop = labels.get(n + 1).map_or(end, |l| l.0);
            let Some(colon) = (label + 1..stop).find(|&k| !t[k].pp && t[k].is(":")) else { continue };
            if !clause_terminated(cx, colon, stop) {
                s.hit(fall_rule, t[label].line, "switch clause falls through");
            }
        }
    }
}

/// Whether the statements in `(colon, stop)` end by leaving the switch.
fn clause_terminated(cx: &FileCtx, colon: usize, stop: usize) -> bool {
    let t = cx.toks();
    let code: Vec<usize> = (colon + 1..stop).filter(|&k| !t[k].pp).collect();
    let Some(&last) = code.last() else {
        return true; // adjacent labels share one clause
    };
    // Look inside a trailing `{ ... }`.
    let mut e = last;
    while t[e].is("}") && cx.pair(e).is_some_and(|o| o > colon) {
        match cx.prev(e) {
            Some(p) if p > colon => e = p,
            _ => return false,
        }
    }
    if !t[e].is(";") {
        return false;
    }
    // Walk back to the head of the last statement.
    let mut k = e;
    let mut head = e;
    while let Some(p) = cx.prev(k) {
        if p <= colon || t[p].is(";") || t[p].is("{") {
            break;
        }
        if t[p].is("}") {
            // `Err{}` is part of the expression; any other `}` ends a statement.
            let init = cx.pair(p).filter(|&o| o > colon).and_then(|o| cx.prev(o).map(|b| (o, b))).filter(|&(_, b)| {
                t[b].is(">") || (t[b].kind == Kind::Ident && !matches!(t[b].text, "else" | "do" | "try"))
            });
            match init {
                Some((o, _)) => {
                    k = o;
                    head = k;
                    continue;
                }
                None => break,
            }
        }
        k = if t[p].is(")") || t[p].is("]") { cx.pair(p).unwrap_or(p) } else { p };
        head = k;
    }
    matches!(t[head].text, "break" | "return" | "continue" | "goto" | "throw" | "__attribute__" | "[")
}

/// The `{` that follows `kw ( ... )`, if the body is a block.
fn body_after_parens(cx: &FileCtx, kw: usize) -> Option<usize> {
    let t = cx.toks();
    let open = cx.next(kw).filter(|&p| t[p].is("("))?;
    let close = cx.pair(open)?;
    cx.next(close).filter(|&b| t[b].is("{"))
}

/// Braced bodies, assignments used as conditions, floating loop counters.
pub(crate) fn control_bodies(s: &mut Sink, braces: &'static Rule, assign: &'static Rule, counter: &'static Rule) {
    let cx = s.cx;
    let t = cx.toks();
    for i in 0..t.len() {
        if t[i].pp || t[i].kind != Kind::Ident {
            continue;
        }
        let kw = t[i].text;
        match kw {
            "else" => {
                if !cx.next(i).is_some_and(|n| t[n].is("{") || t[n].is("if")) {
                    s.hit(braces, t[i].line, "'else' body is not a braced block");
                }
            }
            "do" => {
                if !cx.next(i).is_some_and(|n| t[n].is("{")) {
                    s.hit(braces, t[i].line, "'do' body is not a braced block");
                }
            }
            "if" | "for" | "while" => {
                let mut open = cx.next(i);
                if open.is_some_and(|o| t[o].is("constexpr")) {
                    open = open.and_then(|o| cx.next(o));
                }
                let Some(open) = open.filter(|&o| t[o].is("(")) else { continue };
                let Some(close) = cx.pair(open) else { continue };
                // `} while (...);` closes a do-while: it has no body.
                let do_tail = kw == "while"
                    && cx.prev(i).is_some_and(|p| {
                        t[p].is("}") && cx.pair(p).and_then(|o| cx.prev(o)).is_some_and(|d| t[d].is("do"))
                    });
                if !do_tail && !cx.next(close).is_some_and(|n| t[n].is("{")) {
                    s.hit(braces, t[i].line, format!("'{kw}' body is not a braced block"));
                }
                if kw == "for" {
                    if cx.next(open).is_some_and(|n| matches!(t[n].text, "float" | "double")) {
                        s.hit(counter, t[i].line, "floating-point loop counter");
                    }
                    continue;
                }
                // A bare `=` at the top level of the condition.
                let mut k = open + 1;
                while k < close {
                    if t[k].is("(") || t[k].is("[") || t[k].is("{") {
                        k = cx.pair(k).map_or(k + 1, |p| p + 1);
                        continue;
                    }
                    if t[k].is("=") && !is_declaration(cx, open, k) {
                        s.hit(assign, t[k].line, format!("assignment used as the '{kw}' condition"));
                        break;
                    }
                    k += 1;
                }
            }
            _ => {}
        }
    }
}

/// C++ `if (T x = f())`: a declaration, not an assignment expression.
fn is_declaration(cx: &FileCtx, open: usize, eq: usize) -> bool {
    let t = cx.toks();
    cx.lang == Lang::Cpp
        && eq >= open + 3
        && t[eq - 1].kind == Kind::Ident
        && (t[eq - 2].kind == Kind::Ident || matches!(t[eq - 2].text, "*" | "&" | ">"))
        && t[open + 1].kind == Kind::Ident
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn ids_at(path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::CSubset, &[(path, src)], &Profile::default()).iter().map(|f| f.rule.id).collect()
    }
    fn ids(src: &str) -> Vec<&'static str> {
        ids_at("src/app.c", src)
    }
    /// The rule fires on `bad`, is silent on `good`, and the denial names the clause.
    fn rule(id: &str, clause: &str, bad: &str, good: &str) {
        let p = Profile::default();
        let f = check(Pack::CSubset, &[("src/app.c", bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids(bad));
        let (msg, ev) = render(&f, &p);
        assert!(msg.contains("src/app.c:"), "{msg}");
        assert!(ev.contains(id) && ev.contains(clause), "{ev}");
        assert!(!ids(good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn c001_goto() {
        rule("EMB-C-001", "MISRA C:2012 Rule 15.1; BARR-C:2018 1.7c", "void f(void) { goto out; out: return; }", "void f(void) { return; }");
    }
    #[test]
    fn c002_setjmp() {
        rule("EMB-C-002", "MISRA C:2012 Rule 21.4", "#include <setjmp.h>\n", "#include <stdint.h>\n");
        rule("EMB-C-002", "Rule 21.4", "void f(void) { longjmp(env, 1); }", "void f(void) { s.longjmp = 1; }");
    }
    #[test]
    fn c003_atox() {
        rule("EMB-C-003", "MISRA C:2012 Rule 21.7", "int f(const char *s) { return atoi(s); }", "long f(const char *s) { return strtol(s, &e, 10); }");
    }
    #[test]
    fn c004_termination() {
        rule("EMB-C-004", "MISRA C:2012 Rule 21.8", "void f(void) { exit(1); }", "void f(void) { on_exit_flag = 1; p->exit(1); }");
        assert!(ids("void f(void) { abort(); }").contains(&"EMB-C-004"));
        assert!(ids("char *h(void) { return getenv(\"HOME\"); }").contains(&"EMB-C-004"));
    }
    #[test]
    fn c005_signal() {
        rule("EMB-C-005", "MISRA C:2012 Rule 21.5", "#include <signal.h>\nvoid f(void) { signal(2, h); }", "void f(void) { int signal = 1; }");
    }
    #[test]
    fn c006_stdarg() {
        rule("EMB-C-006", "MISRA C:2012 Rule 17.1", "#include <stdarg.h>\n", "#include <stddef.h>\n");
        assert!(ids("void f(int n, va_list ap) { va_start(ap, n); va_end(ap); }").contains(&"EMB-C-006"));
    }
    #[test]
    fn c007_stdio() {
        rule("EMB-C-007", "MISRA C:2012 Rule 21.6", "#include <stdio.h>\n", "#include \"my_stdio_shim.h\"\n");
    }
    #[test]
    fn c008_octal() {
        rule("EMB-C-008", "MISRA C:2012 Rule 7.1", "int m = 010;", "int m = 0; int h = 0x10; float f = 0.5f; double d = 00.5; int z = 10;");
        assert!(ids("#define MODE 0644\n").contains(&"EMB-C-008"));
    }
    #[test]
    fn c009_nested_comment() {
        rule("EMB-C-009", "MISRA C:2012 Rule 3.1; BARR-C:2018 2.1b", "/* outer /* inner */\nint a;", "/* fine */ int a; // also /* fine\n");
    }
    #[test]
    fn c010_reserved_macro() {
        rule("EMB-C-010", "MISRA C:2012 Rule 21.1", "#define __APP_H__\n", "#define APP_H\n#define _lower 1\n");
        assert!(ids("#define _Reserved 1\n").contains(&"EMB-C-010"));
    }
    #[test]
    fn c011_switch_default() {
        rule(
            "EMB-C-011",
            "MISRA C:2012 Rule 16.4; BARR-C:2018 8.3b",
            "void f(int x) { switch (x) { case 1: g(); break; } }",
            "void f(int x) { switch (x) { case 1: g(); break; default: break; } }",
        );
        // A nested switch's default does not count for the outer one.
        let nested = "void f(int x) { switch (x) { case 1: { switch (y) { default: break; } break; } } }";
        assert_eq!(ids(nested).iter().filter(|i| **i == "EMB-C-011").count(), 1);
    }
    #[test]
    fn c012_fallthrough() {
        rule(
            "EMB-C-012",
            "MISRA C:2012 Rule 16.3",
            "void f(int x) { switch (x) { case 1: g(); case 2: h(); break; default: break; } }",
            "int f(int x) { switch (x) { case 1: case 2: g(); break; case 3: { h(); break; } case 4: return x ? 1 : 2; default: break; } return 0; }",
        );
        assert!(ids("void f(int x) { switch (x) { case 1: g(); break; default: g(); } }").contains(&"EMB-C-012"));
    }
    #[test]
    fn c013_braces() {
        rule("EMB-C-013", "MISRA C:2012 Rule 15.6; BARR-C:2018 1.3a", "void f(void) { if (a) g(); }", "void f(void) { if (a) { g(); } else if (b) { h(); } else { k(); } }");
        assert!(ids("void f(void) { for (i = 0; i < 3; i++) g(); }").contains(&"EMB-C-013"));
        assert!(ids("void f(void) { while (a) g(); }").contains(&"EMB-C-013"));
        assert!(ids("void f(void) { if (a) { g(); } else h(); }").contains(&"EMB-C-013"));
        assert!(ids("void f(void) { do g(); while (a); }").contains(&"EMB-C-013"));
        assert!(!ids("void f(void) { do { g(); } while (a); }").contains(&"EMB-C-013"));
        // A macro body is not a statement.
        assert!(!ids("#define CHECK(x) if (x) fail()\n").contains(&"EMB-C-013"));
    }
    #[test]
    fn c014_assign_in_condition() {
        rule("EMB-C-014", "MISRA C:2012 Rule 13.4; BARR-C:2018 8.2c", "void f(void) { if (a = b) { g(); } }", "void f(void) { if (a == b) { g(); } if (a <= b) { g(); } if ((c = get()) != 0) { g(); } }");
        assert!(ids("void f(void) { while (p = next(p)) { g(); } }").contains(&"EMB-C-014"));
    }
    #[test]
    fn c015_float_equality() {
        rule("EMB-C-015", "BARR-C:2018 5.4b.iv", "int f(float x) { return x == 0.0f; }", "int f(int x, float y) { return x == 0 && y < 1.5f && x != 0x1E; }");
        assert!(ids("int f(double x) { return 1e-3 != x; }").contains(&"EMB-C-015"));
    }
    #[test]
    fn c016_float_loop_counter() {
        rule("EMB-C-016", "MISRA C:2012 Rule 14.1", "void f(void) { for (float x = 0.0f; x < 1.0f; x += 0.1f) { g(); } }", "void f(void) { for (int i = 0; i < 3; i++) { g(); } }");
    }
    #[test]
    fn c017_macro_paste_variadic() {
        rule("EMB-C-017", "MISRA C:2012 Rule 20.10", "#define CAT(a, b) a ## b\n", "#define STR(a) #a\n#define ADD(a, b) ((a) + (b))\n");
        assert!(ids("#define LOG(fmt, ...) log_write(fmt, __VA_ARGS__)\n").contains(&"EMB-C-017"));
    }
    #[test]
    fn c018_register_auto() {
        rule("EMB-C-018", "BARR-C:2018 1.7a, 1.7b", "void f(void) { register int i = 0; }", "void f(void) { int register_count = 0; }");
        assert!(ids("void f(void) { auto int i = 0; }").contains(&"EMB-C-018"));
    }
    #[test]
    fn c019_bsearch_qsort() {
        rule("EMB-C-019", "MISRA C:2012 Rule 21.9", "void f(void) { qsort(a, n, sizeof a[0], cmp); }", "void f(void) { my_qsort(a, n); }");
    }

    #[test]
    fn c_subset_ignores_comments_and_strings() {
        let src = "/* goto x; if (a) b(); 010 */\n// exit(1); #include <stdio.h>\nconst char *s = \"goto atoi(x) 010 a ## b if (a = b)\";\nchar c = '0';\n";
        assert_eq!(ids(src), Vec::<&str>::new());
    }
    #[test]
    fn c_subset_exempts_tests_and_third_party() {
        let bad = "#include <stdio.h>\nvoid f(void) { goto x; x: exit(1); }";
        assert!(!ids(bad).is_empty());
        for p in ["tests/app_test.c", "test/unit.c", "src/test_app.c", "third_party/lib/x.c", "vendor/x.c"] {
            assert_eq!(ids_at(p, bad), Vec::<&str>::new(), "{p}");
        }
        // Not C: left to the other packs.
        assert_eq!(ids_at("src/app.cpp", bad), Vec::<&str>::new());
        assert_eq!(ids_at("docs/notes.md", bad), Vec::<&str>::new());
    }
    #[test]
    fn c_subset_clean_file_passes() {
        let src = r#"
#include <stdint.h>
#include "motor.h"

#define MOTOR_MAX_RPM (6000U)

/* Clamp a requested speed to the motor's limit. */
static uint32_t clamp_rpm(uint32_t rpm)
{
    uint32_t out = rpm;
    if (rpm > MOTOR_MAX_RPM)
    {
        out = MOTOR_MAX_RPM;
    }
    else
    {
        out = rpm;
    }
    return out;
}

uint32_t motor_step(motor_state_t state, uint32_t rpm)
{
    uint32_t next = 0U;
    for (uint32_t i = 0U; i < 4U; i++)
    {
        next += clamp_rpm(rpm);
    }
    switch (state)
    {
        case MOTOR_IDLE:
        case MOTOR_STOPPED:
            next = 0U;
            break;
        case MOTOR_RUNNING:
        {
            next = clamp_rpm(rpm);
            break;
        }
        default:
            next = 0U;
            break;
    }
    do
    {
        next--;
    } while (next > MOTOR_MAX_RPM);
    return next;
}
"#;
        assert_eq!(ids(src), Vec::<&str>::new());
    }

    #[test]
    fn a_justified_deviation_waives_a_required_rule_on_its_line_only() {
        let src = "void f(void) {\n  /* EMB-DEVIATION(EMB-C-001): single error exit, reviewed in DR-12 */\n  goto out;\n  goto out;\nout: return; }";
        assert_eq!(ids(src), ["EMB-C-001"]);
        let short = "void f(void) {\n  goto out; /* EMB-DEVIATION(EMB-C-001): because */\nout: return; }";
        assert_eq!(ids(short), ["EMB-C-001"]);
    }
    #[test]
    fn a_strict_rule_is_not_deviatable_on_a_high_integrity_profile() {
        let src = "void f(void) {\n  goto out; /* EMB-DEVIATION(EMB-C-001): single error exit, reviewed in DR-12 */\nout: return; }";
        let asil_d = Profile::parse("[safety]\niso26262_asil = \"D\"").unwrap();
        assert_eq!(check(Pack::CSubset, &[("src/a.c", src)], &asil_d).len(), 1);
        assert!(check(Pack::CSubset, &[("src/a.c", src)], &Profile::default()).is_empty());
    }
    #[test]
    fn malformed_input_never_panics() {
        for src in ["switch (", "if (a", "} } ) switch (x) { case", "#define", "#", "for (;;", "do", "else", "case 1:", "switch (x) { case 1: }"] {
            let _ = ids(src);
        }
    }
}
