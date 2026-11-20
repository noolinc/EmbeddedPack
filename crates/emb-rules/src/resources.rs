//! `emb_resource_bounds`: memory and execution must be bounded and
//! predictable. The heap rule follows `architecture.heap` in the hardware
//! profile; the function-length limit follows `architecture.max_function_lines`.
//!
//! Clauses were checked against the JSF AV and AUTOSAR C++14 texts, the free
//! MISRA documents, and the published Power of 10 rules.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, FileCtx, Func, Lang, Pack, Rule};
use crate::profile::Heap;

macro_rules! rules {
    ($( $name:ident = $id:literal, $strict:literal, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Resources, category: Category::Required, strict: $strict, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    HEAP = "EMB-RES-001", true, "dynamic memory shall not be used beyond what the hardware profile allows",
        "MISRA C:2012 Dir 4.12, Rule 21.3; Power of 10 Rule 3; JSF AV Rule 206; AUTOSAR C++14 A18-5-1, A18-5-2";
    RECURSION = "EMB-RES-002", true, "functions shall not call themselves",
        "MISRA C:2012 Rule 17.2; Power of 10 Rule 1; JSF AV Rule 119; AUTOSAR C++14 A7-5-2";
    VLA = "EMB-RES-003", true, "alloca and variable-length arrays shall not be used",
        "MISRA C:2012 Rule 18.8; SEI CERT C ARR32-C";
    UNBOUNDED_LOOP = "EMB-RES-004", false, "every loop shall have a fixed upper bound, except a task's main loop",
        "Power of 10 Rule 2";
    FUNCTION_LENGTH = "EMB-RES-005", false, "a function shall fit on one page",
        "Power of 10 Rule 4";
    POINTER_DEPTH = "EMB-RES-006", false, "no more than two levels of pointer indirection",
        "MISRA C:2012 Rule 18.5; JSF AV Rule 170";
}

const ALLOC: [&str; 22] = [
    "malloc", "calloc", "realloc", "aligned_alloc", "strdup", "strndup", "pvPortMalloc", "pvPortCalloc", "k_malloc", "k_calloc",
    "k_aligned_alloc", "os_malloc", "tx_byte_allocate", "rt_malloc", "heap_caps_malloc", "heap_caps_calloc", "make_unique",
    "make_shared", "asprintf", "vasprintf", "memalign", "posix_memalign",
];
const FREE: [&str; 7] = ["free", "vPortFree", "k_free", "os_free", "tx_byte_release", "rt_free", "heap_caps_free"];

/// Functions that run once at start-up, where an init-only heap may allocate.
pub(crate) fn is_init_like(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "main" || ["init", "setup", "boot", "startup"].iter().any(|k| n.contains(k))
}

/// Functions whose job is to run forever.
fn may_loop_forever(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["main", "task", "thread", "loop", "idle", "fault", "panic", "halt", "hang", "error", "abort", "assert", "reset", "worker", "daemon", "entry", "run", "handler"]
        .iter()
        .any(|k| n.contains(k))
}

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if !cx.is_c_family() || !cx.is_production() {
        return;
    }
    heap(s);
    functions(s);
    stack_and_pointers(s);
}

fn heap(s: &mut Sink) {
    let cx = s.cx;
    let mode = cx.profile.heap;
    if mode == Heap::Allowed {
        return;
    }
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.pp || tok.kind != Kind::Ident {
            continue;
        }
        let prev = cx.prev(i).map(|p| t[p].text);
        let next = cx.next(i).map(|n| t[n].text);
        let (what, releases) = match tok.text {
            n if ALLOC.contains(&n) && cx.is_call(i) => (format!("{n}()"), false),
            n if FREE.contains(&n) && cx.is_call(i) => (format!("{n}()"), true),
            // `make_unique<T>(...)`: the template arguments hide the call.
            "make_unique" | "make_shared" if next == Some("<") => (format!("std::{}", tok.text), false),
            "new" if cx.lang == Lang::Cpp && next != Some("(") && prev != Some("operator") => ("new expression".to_string(), false),
            "delete" if cx.lang == Lang::Cpp && prev != Some("=") && prev != Some("operator") => ("delete expression".to_string(), true),
            _ => continue,
        };
        let in_init = cx.func_at(i).is_some_and(|f| is_init_like(f.name));
        match mode {
            Heap::None => s.hit(&HEAP, tok.line, format!("{what}: the profile has no heap")),
            Heap::InitOnly if releases => s.hit(&HEAP, tok.line, format!("{what}: an init-only heap is never freed")),
            Heap::InitOnly if !in_init => s.hit(&HEAP, tok.line, format!("{what} after start-up: allocate only in init functions")),
            _ => {}
        }
    }
}

/// Arguments in the parenthesis at `open`; `(void)` and `()` are zero.
fn arg_count(cx: &FileCtx, open: usize) -> usize {
    let t = cx.toks();
    let Some(close) = cx.pair(open) else { return 0 };
    let inner: Vec<usize> = (open + 1..close).filter(|&k| !t[k].pp).collect();
    if inner.is_empty() || (inner.len() == 1 && t[inner[0]].is("void")) {
        return 0;
    }
    let mut n = 1;
    let mut k = open + 1;
    while k < close {
        if t[k].is("(") || t[k].is("[") || t[k].is("{") {
            k = cx.pair(k).map_or(k + 1, |p| p + 1);
            continue;
        }
        if t[k].is(",") {
            n += 1;
        }
        k += 1;
    }
    n
}

fn functions(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let max = cx.profile.max_function_lines;
    for f in &cx.funcs {
        let lines = f.end_line.saturating_sub(t[f.open].line) + 1;
        if max > 0 && lines > max {
            s.hit(&FUNCTION_LENGTH, f.line, format!("{}() body spans {lines} lines; the limit is {max}", f.name));
        }
        recursion(s, f);
        loops(s, f);
    }
}

fn recursion(s: &mut Sink, f: &Func) {
    let cx = s.cx;
    let t = cx.toks();
    let params = cx.next(f.name_tok).map_or(0, |o| arg_count(cx, o));
    for k in f.open + 1..f.close {
        if t[k].kind != Kind::Ident || t[k].text != f.name || !cx.is_call(k) {
            continue;
        }
        // `Base::f()` from an override is another function.
        if cx.prev(k).is_some_and(|p| t[p].is("::")) {
            continue;
        }
        // A C++ overload that delegates takes a different number of arguments.
        let args = cx.next(k).map_or(0, |o| arg_count(cx, o));
        if cx.lang == Lang::Cpp && args != params {
            continue;
        }
        s.hit(&RECURSION, t[k].line, format!("{}() calls itself", f.name));
        return;
    }
}

fn loops(s: &mut Sink, f: &Func) {
    let cx = s.cx;
    if may_loop_forever(f.name) {
        return;
    }
    let t = cx.toks();
    for k in f.open + 1..f.close {
        if t[k].pp || t[k].kind != Kind::Ident {
            continue;
        }
        let Some(open) = cx.next(k).filter(|&o| t[o].is("(")) else { continue };
        let Some(close) = cx.pair(open) else { continue };
        let inner: Vec<&str> = (open + 1..close).filter(|&j| !t[j].pp).map(|j| t[j].text).collect();
        let forever = match t[k].text {
            "while" => matches!(inner.as_slice(), ["1"] | ["true"] | ["TRUE"] | ["1U"] | ["1u"]),
            "for" => matches!(inner.as_slice(), [";", ";"]),
            _ => false,
        };
        if forever {
            s.hit(&UNBOUNDED_LOOP, t[k].line, format!("unbounded loop in {}()", f.name));
        }
    }
}

const NOT_A_DECLARATION: [&str; 12] = ["return", "goto", "case", "else", "new", "delete", "throw", "static", "extern", "sizeof", "typedef", "do"];

fn stack_and_pointers(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.pp {
            continue;
        }
        if tok.kind == Kind::Ident && matches!(tok.text, "alloca" | "__builtin_alloca" | "_alloca") && cx.is_call(i) {
            s.hit(&VLA, tok.line, format!("{}()", tok.text));
        }
        // `T ***p`: three stars after a type name.
        if tok.is("*") && i >= 3 && t[i - 1].is("*") && t[i - 2].is("*") && t[i - 3].kind == Kind::Ident && !t[i - 3].pp {
            if !(i + 1 < t.len() && t[i + 1].is("*")) {
                s.hit(&POINTER_DEPTH, tok.line, "more than two levels of pointer indirection");
            }
        }
        if cx.lang == Lang::C && tok.is("[") && is_vla(cx, i) {
            s.hit(&VLA, tok.line, format!("variable-length array '{}'", t[i - 1].text));
        }
    }
}

/// `type name[expr];` inside a function, where `expr` names a variable.
fn is_vla(cx: &FileCtx, open: usize) -> bool {
    let t = cx.toks();
    if cx.func_at(open).is_none() {
        return false;
    }
    let Some(close) = cx.pair(open) else { return false };
    let Some(name) = cx.prev(open).filter(|&n| t[n].kind == Kind::Ident) else { return false };
    // Everything back to the start of the statement must read as a type.
    let mut words = 0;
    let mut k = name;
    while let Some(p) = cx.prev(k) {
        if t[p].is(";") || t[p].is("{") || t[p].is("}") {
            break;
        }
        match t[p].kind {
            Kind::Ident if !NOT_A_DECLARATION.contains(&t[p].text) => words += 1,
            Kind::Punct if t[p].text == "*" => {}
            _ => return false,
        }
        k = p;
    }
    if words == 0 || !cx.next(close).is_some_and(|n| t[n].is(";") || t[n].is("[")) {
        return false;
    }
    // A size that names something not written like a constant. Arguments of
    // `sizeof(...)` and of macro calls are not sizes themselves.
    let mut j = open + 1;
    while j < close {
        if t[j].kind == Kind::Ident {
            if let Some(args) = cx.next(j).filter(|&n| t[n].is("(")) {
                j = cx.pair(args).map_or(j + 1, |c| c + 1);
                continue;
            }
            if t[j].text != "sizeof" && t[j].text.chars().any(|c| c.is_ascii_lowercase()) {
                return true;
            }
        }
        j += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn prof(heap: &str) -> Profile {
        Profile::parse(&format!("[architecture]\nheap = \"{heap}\"")).unwrap()
    }
    fn ids_with(p: &Profile, path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Resources, &[(path, src)], p).iter().map(|f| f.rule.id).collect()
    }
    fn ids(src: &str) -> Vec<&'static str> {
        ids_with(&Profile::default(), "src/app.c", src)
    }
    fn cites(src: &str, id: &str, clause: &str) {
        let p = Profile::default();
        let f = check(Pack::Resources, &[("src/app.c", src)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on {src}: {:?}", ids(src));
        let (_, ev) = render(&f, &p);
        assert!(ev.contains(clause), "{ev}");
    }

    #[test]
    fn res001_dynamic_allocation() {
        let alloc = "void work(void) { char *p = malloc(16); free(p); }";
        cites(alloc, "EMB-RES-001", "MISRA C:2012 Dir 4.12, Rule 21.3; Power of 10 Rule 3; JSF AV Rule 206; AUTOSAR C++14 A18-5-1, A18-5-2");
        assert_eq!(ids(alloc), ["EMB-RES-001", "EMB-RES-001"]);
        for bad in ["void f(void) { void *p = pvPortMalloc(8); }", "void f(void) { k_free(p); }", "void f(void) { char *s = strdup(x); }"] {
            assert!(ids(bad).contains(&"EMB-RES-001"), "{bad}");
        }
        // C++ forms.
        let p = Profile::default();
        for bad in ["void f() { auto p = new Foo(); }", "void f() { delete p; }", "void f() { auto p = std::make_unique<Foo>(); }"] {
            assert!(ids_with(&p, "src/a.cpp", bad).contains(&"EMB-RES-001"), "{bad}");
        }
        // The profile decides.
        assert!(ids_with(&prof("allowed"), "src/app.c", alloc).is_empty());
        let init_only = prof("init-only");
        assert!(ids_with(&init_only, "src/app.c", "void uart_init(void) { buf = malloc(64); }").is_empty());
        assert_eq!(ids_with(&init_only, "src/app.c", "void uart_rx(void) { buf = malloc(64); }"), ["EMB-RES-001"]);
        assert_eq!(ids_with(&init_only, "src/app.c", "void uart_init(void) { free(buf); }"), ["EMB-RES-001"]);
    }
    #[test]
    fn res002_recursion() {
        cites("int fact(int n) { return (n < 2) ? 1 : n * fact(n - 1); }", "EMB-RES-002", "MISRA C:2012 Rule 17.2; Power of 10 Rule 1; JSF AV Rule 119; AUTOSAR C++14 A7-5-2");
        assert!(!ids("int twice(int n) { return helper(n) * 2; } int helper(int n) { return n; }").contains(&"EMB-RES-002"));
    }
    #[test]
    fn res003_alloca_vla() {
        cites("void f(int n) { char *p = alloca(n); }", "EMB-RES-003", "MISRA C:2012 Rule 18.8; SEI CERT C ARR32-C");
        assert!(ids("void f(size_t n) { uint8_t buf[n]; use(buf); }").contains(&"EMB-RES-003"));
        assert!(ids("void f(size_t len) { uint8_t buf[len + 1]; use(buf); }").contains(&"EMB-RES-003"));
        for ok in [
            "void f(void) { uint8_t buf[16]; uint8_t b2[BUF_SIZE]; uint8_t b3[sizeof(hdr_t)]; uint8_t b4[ARRAY_SIZE(x)]; }",
            "void f(int i) { buf[i] = 0; x = table[idx]; return arr[i]; }",
            "void f(void) { static uint8_t pool[n]; }",
            "uint8_t global[count];",
        ] {
            assert!(!ids(ok).contains(&"EMB-RES-003"), "{ok}");
        }
    }
    #[test]
    fn res004_unbounded_loop() {
        cites("void poll(void) { while (1) { step(); } }", "EMB-RES-004", "Power of 10 Rule 2");
        assert!(ids("void poll(void) { for (;;) { step(); } }").contains(&"EMB-RES-004"));
        assert!(!ids("void poll(void) { for (uint32_t i = 0U; i < 8U; i++) { step(); } while (n > 0U) { n--; } }").contains(&"EMB-RES-004"));
    }
    #[test]
    fn res005_function_length() {
        let body = "    x++;\n".repeat(70);
        let long = format!("void big(void)\n{{\n{body}}}\n");
        cites(&long, "EMB-RES-005", "Power of 10 Rule 4");
        let short = format!("void small(void)\n{{\n{}}}\n", "    x++;\n".repeat(20));
        assert!(ids(&short).is_empty());
        // The limit is the profile's.
        let p = Profile::parse("[architecture]\nmax_function_lines = 10").unwrap();
        assert_eq!(ids_with(&p, "src/app.c", &short), ["EMB-RES-005"]);
    }
    #[test]
    fn res006_pointer_depth() {
        cites("void f(char ***argv);", "EMB-RES-006", "MISRA C:2012 Rule 18.5; JSF AV Rule 170");
        assert!(!ids("void f(char **argv); int x = a * *p;").contains(&"EMB-RES-006"));
    }

    #[test]
    fn resources_does_not_flag_bounded_code() {
        let c = r#"
#include <stdint.h>
static uint8_t pool[256];
int main(void)
{
    board_init();
    for (;;)
    {
        scheduler_step();
    }
}
void sensor_task(void *arg)
{
    while (1)
    {
        read(arg);
    }
}
void use(struct heap *h) { h->free(h); obj.malloc(3); }
"#;
        assert_eq!(ids(c), Vec::<&str>::new());
        let cpp = r#"
class Buf {
public:
    Buf(const Buf &) = delete;
    void *operator new(std::size_t) = delete;
    void put(int v) { put(v, 0); }
    void put(int v, int at) { data_[at] = v; }
    void build() { auto *o = new (storage_) Obj(); (void)o; }
    void reset() override { Base::reset(); }
};
"#;
        assert_eq!(ids_with(&Profile::default(), "src/buf.cpp", cpp), Vec::<&str>::new());
    }
    #[test]
    fn resources_scope() {
        let bad = "int fact(int n) { char *p = malloc(4); return fact(n - 1); }";
        assert!(!ids(bad).is_empty());
        for p in ["tests/alloc_test.c", "third_party/x.c", "docs/x.md", "Makefile"] {
            assert_eq!(ids_with(&Profile::default(), p, bad), Vec::<&str>::new(), "{p}");
        }
    }
    #[test]
    fn resources_strict_rules_on_high_integrity() {
        let src = "void f(void) {\n  /* EMB-DEVIATION(EMB-RES-001): start-up pool, reviewed in DR-3 */\n  p = malloc(8);\n}\nint g(int n) {\n  /* EMB-DEVIATION(EMB-RES-002): depth bounded by tree height 4 */\n  return g(n - 1);\n}";
        assert!(ids_with(&Profile::default(), "src/a.c", src).is_empty());
        for level in ["[safety]\niso26262_asil = \"C\"", "[safety]\niec62304_class = \"C\"", "[safety]\niec61508_sil = 3"] {
            let p = Profile::parse(level).unwrap();
            assert_eq!(ids_with(&p, "src/a.c", src), ["EMB-RES-001", "EMB-RES-002"], "{level}");
        }
    }
    #[test]
    fn malformed_resources_input_never_panics() {
        for src in ["void f(", "f() {", "int a[", "***", "while (", "for (;;", "new", "void f(void) { x[ }"] {
            let _ = ids(src);
            let _ = ids_with(&Profile::default(), "a.cpp", src);
        }
    }
}
