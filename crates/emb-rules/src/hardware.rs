//! `emb_hardware_fit`: code must fit the hardware and architecture in
//! `hardware-profile.toml`. The same source can be right on one board and
//! wrong on another, so every rule here reads the profile.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, FileCtx, Func, Lang, Pack, Rule, file_name};
use crate::profile::{Exec, Fpu, Heap, Profile};
use crate::resources::is_init_like;

macro_rules! rules {
    ($( $name:ident = $id:literal, $cat:ident, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Hardware, category: Category::$cat, strict: false, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    STACK_OBJECT = "EMB-HW-001", Required, "a stack object shall fit the target's RAM budget",
        "hardware profile: hardware.ram_bytes, architecture.max_stack_object_bytes";
    FLOAT = "EMB-HW-002", Required, "floating point shall match the target's FPU",
        "hardware profile: hardware.fpu; BARR-C:2018 5.4a";
    FLASH_LIB = "EMB-HW-003", Required, "stream and float-formatting libraries shall not be used on small flash",
        "hardware profile: hardware.flash_bytes";
    EXCEPTIONS = "EMB-HW-004", Required, "exceptions and RTTI shall not be used without a heap or on small flash",
        "hardware profile: architecture.heap, hardware.flash_bytes; JSF AV Rule 208";
    EXEC_MODEL = "EMB-HW-005", Required, "code shall follow the declared execution model",
        "hardware profile: architecture.execution_model";
    PROFILE = "EMB-HW-006", Mandatory, "hardware-profile.toml shall parse",
        "hardware profile";
}

pub const PROFILE_FILE: &str = "hardware-profile.toml";

const BUSY_DELAY: [&str; 9] = ["HAL_Delay", "delay", "_delay_ms", "mdelay", "k_busy_wait", "sleep", "usleep", "msleep", "nanosleep"];
const KERNEL_SLEEP: [&str; 10] = [
    "vTaskDelay", "vTaskDelayUntil", "xTaskDelayUntil", "k_sleep", "k_msleep", "k_usleep", "osDelay", "osDelayUntil", "tx_thread_sleep",
    "rt_thread_mdelay",
];
const KERNEL_API: [&str; 10] = [
    "xTaskCreate", "xTaskCreateStatic", "xTaskCreatePinnedToCore", "vTaskStartScheduler", "k_thread_create", "osThreadNew",
    "osKernelStart", "pthread_create", "tx_thread_create", "rt_thread_create",
];
const WAIT_FOREVER: [&str; 5] = ["portMAX_DELAY", "K_FOREVER", "osWaitForever", "TX_WAIT_FOREVER", "RT_WAITING_FOREVER"];
const FLASH_HEAVY: [&str; 6] = ["iostream", "sstream", "fstream", "regex", "locale", "iomanip"];

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if file_name(cx.path) == PROFILE_FILE {
        if let Err(e) = Profile::parse(cx.text) {
            s.hit(&PROFILE, 0, e);
        }
        return;
    }
    if !cx.is_c_family() || !cx.is_production() {
        return;
    }
    stack_objects(s);
    floating_point(s);
    flash_and_exceptions(s);
    execution_model(s);
}

fn elem_size(ty: &str) -> u64 {
    match ty {
        "char" | "bool" | "_Bool" | "uint8_t" | "int8_t" | "u8" | "s8" | "byte" => 1,
        "short" | "uint16_t" | "int16_t" | "u16" | "s16" | "wchar_t" => 2,
        "double" | "uint64_t" | "int64_t" | "u64" | "s64" => 8,
        "int" | "long" | "unsigned" | "float" | "uint32_t" | "int32_t" | "u32" | "s32" | "size_t" => 4,
        // A struct is at least one byte per element: a lower bound.
        _ => 1,
    }
}

pub(crate) fn int_value(n: &str) -> Option<u64> {
    let d = n.trim_end_matches(['u', 'U', 'l', 'L']).replace('\'', "");
    match d.strip_prefix("0x").or_else(|| d.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => d.parse().ok(),
    }
}

/// `type name[N]...;` inside a function, not `static`: it lives on the stack.
fn stack_objects(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let limit = cx.profile.stack_object_limit();
    for (i, tok) in t.iter().enumerate() {
        if tok.pp || !tok.is("[") || cx.func_at(i).is_none() {
            continue;
        }
        let Some(name) = cx.prev(i).filter(|&n| t[n].kind == Kind::Ident) else { continue };
        // The declaration's words, back to the start of the statement.
        let mut ty: Option<&str> = None;
        let mut k = name;
        let mut ok = true;
        while let Some(p) = cx.prev(k) {
            if t[p].is(";") || t[p].is("{") || t[p].is("}") {
                break;
            }
            match t[p].kind {
                Kind::Ident if matches!(t[p].text, "static" | "extern" | "return" | "typedef" | "sizeof" | "case" | "goto" | "else") => ok = false,
                Kind::Ident => {
                    if ty.is_none() && !matches!(t[p].text, "const" | "volatile" | "unsigned" | "signed" | "struct") {
                        ty = Some(t[p].text);
                    } else if ty.is_none() && matches!(t[p].text, "unsigned" | "signed") {
                        ty = Some("int");
                    }
                }
                _ => ok = false,
            }
            k = p;
        }
        let Some(ty) = ty.filter(|_| ok) else { continue };
        // Multiply every literal dimension.
        let mut bytes = elem_size(ty);
        let mut open = i;
        loop {
            let Some(close) = cx.pair(open) else { break };
            let dims: Vec<usize> = (open + 1..close).filter(|&j| !t[j].pp).collect();
            match dims.as_slice() {
                [d] if t[*d].kind == Kind::Number => match int_value(t[*d].text) {
                    Some(n) => bytes = bytes.saturating_mul(n),
                    None => break,
                },
                _ => {
                    bytes = 0;
                    break;
                }
            }
            match cx.next(close) {
                Some(n) if t[n].is("[") => open = n,
                _ => break,
            }
        }
        if bytes > limit {
            s.hit(&STACK_OBJECT, tok.line, format!("'{}' puts at least {bytes} bytes on the stack; this target allows {limit} (RAM {} bytes)", t[name].text, cx.profile.ram_bytes));
        }
    }
}

fn floating_point(s: &mut Sink) {
    let cx = s.cx;
    let fpu = cx.profile.fpu;
    if fpu == Fpu::Double {
        return;
    }
    let mut shown = 0;
    for tok in cx.toks() {
        if tok.kind != Kind::Ident || shown >= 3 {
            continue;
        }
        let bad = match (fpu, tok.text) {
            (Fpu::None, "float" | "double") => Some("this target has no FPU; use fixed-point"),
            (Fpu::Single, "double") => Some("this target's FPU is single-precision; double runs in software"),
            _ => None,
        };
        if let Some(why) = bad {
            s.hit(&FLOAT, tok.line, format!("'{}': {why}", tok.text));
            shown += 1;
        }
    }
}

/// `%f`, `%.2f`, `%8.3e`, `%lg` ... in a format string.
fn has_float_format(body: &str) -> bool {
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_digit() || matches!(b[j], b'.' | b'-' | b'+' | b' ' | b'#' | b'l' | b'L' | b'*')) {
                j += 1;
            }
            if j < b.len() && j > i && matches!(b[j], b'f' | b'F' | b'e' | b'E' | b'g' | b'G') {
                return true;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    false
}

fn flash_and_exceptions(s: &mut Sink) {
    let cx = s.cx;
    let small = cx.profile.small_flash();
    let no_unwind = small || cx.profile.heap == Heap::None;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if small && tok.kind == Kind::Hash && t.get(i + 1).is_some_and(|d| d.is("include")) {
            if let Some(h) = t.get(i + 2).and_then(|h| h.str_body()).filter(|h| FLASH_HEAVY.contains(h)) {
                s.hit(&FLASH_LIB, tok.line, format!("<{h}> does not fit {} bytes of flash", cx.profile.flash_bytes));
            }
        }
        if small && tok.kind == Kind::Str && !tok.pp && tok.str_body().is_some_and(has_float_format) {
            s.hit(&FLASH_LIB, tok.line, format!("floating-point format pulls the float printf library into {} bytes of flash", cx.profile.flash_bytes));
        }
        if no_unwind && cx.lang == Lang::Cpp && tok.kind == Kind::Ident && !tok.pp && matches!(tok.text, "throw" | "try" | "catch" | "dynamic_cast" | "typeid") {
            let why = if small { "flash is too small for unwind tables and RTTI" } else { "exceptions allocate and the profile has no heap" };
            s.hit(&EXCEPTIONS, tok.line, format!("'{}': {why}", tok.text));
        }
    }
}

fn is_event_handler(f: &Func) -> bool {
    let n = f.name;
    !f.is_isr
        && (n.starts_with("on_") || n.starts_with("handle_") || n.starts_with("On") || n.starts_with("Handle")
            || n.ends_with("_handler") || n.ends_with("_cb") || n.ends_with("_callback") || n.ends_with("Callback"))
}

fn execution_model(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let exec = cx.profile.exec;
    for f in &cx.funcs {
        let init = is_init_like(f.name) && f.name != "main";
        let handler = is_event_handler(f);
        for k in f.open + 1..f.close {
            if t[k].pp || t[k].kind != Kind::Ident {
                continue;
            }
            let name = t[k].text;
            let call = cx.is_call(k);
            let msg = match exec {
                Exec::Superloop if call && (KERNEL_SLEEP.contains(&name) || KERNEL_API.contains(&name)) => {
                    Some(format!("{name}(): the profile is a superloop with no kernel"))
                }
                Exec::Superloop if call && BUSY_DELAY.contains(&name) && !init => {
                    Some(format!("{name}() blocks the main loop; use a timer and a state machine"))
                }
                Exec::Rtos if call && BUSY_DELAY.contains(&name) && !init => {
                    Some(format!("{name}() busy-waits and starves lower-priority tasks; block in the kernel"))
                }
                Exec::EventDriven if handler && call && (BUSY_DELAY.contains(&name) || KERNEL_SLEEP.contains(&name)) => {
                    Some(format!("{name}() blocks in event handler {}(); handlers run to completion", f.name))
                }
                Exec::EventDriven if handler && WAIT_FOREVER.contains(&name) => {
                    Some(format!("{name} waits forever in event handler {}()", f.name))
                }
                Exec::EventDriven if handler && endless_loop(cx, k) => Some(format!("endless loop in event handler {}()", f.name)),
                _ => None,
            };
            if let Some(m) = msg {
                s.hit(&EXEC_MODEL, t[k].line, m);
            }
        }
    }
}

fn endless_loop(cx: &FileCtx, k: usize) -> bool {
    let t = cx.toks();
    let Some(open) = cx.next(k).filter(|&o| t[o].is("(")) else { return false };
    let Some(close) = cx.pair(open) else { return false };
    let inner: Vec<&str> = (open + 1..close).map(|j| t[j].text).collect();
    match t[k].text {
        "while" => matches!(inner.as_slice(), ["1"] | ["true"]),
        "for" => matches!(inner.as_slice(), [";", ";"]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn prof(toml: &str) -> Profile {
        Profile::parse(toml).unwrap()
    }
    fn ids(p: &Profile, path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Hardware, &[(path, src)], p).iter().map(|f| f.rule.id).collect()
    }
    const TINY: &str = "[hardware]\nname = \"attiny\"\nram_bytes = 2k\nflash_bytes = 32k\nfpu = \"none\"\n[architecture]\nexecution_model = \"superloop\"\nheap = \"none\"";
    const BIG: &str = "[hardware]\nname = \"cortex-a\"\nram_bytes = 64m\nflash_bytes = 16m\nfpu = \"double\"\n[architecture]\nexecution_model = \"rtos\"\nheap = \"allowed\"";

    #[test]
    fn hw001_stack_object() {
        let src = "void f(void) { uint8_t frame[1024]; uint32_t small[4]; use(frame, small); }";
        let tiny = prof(TINY);
        assert_eq!(tiny.stack_object_limit(), 64);
        let f = check(Pack::Hardware, &[("src/a.c", src)], &tiny);
        assert_eq!(f.len(), 1);
        assert!(f[0].detail.contains("1024 bytes") && f[0].detail.contains("allows 64"), "{}", f[0].detail);
        assert!(render(&f, &tiny).1.contains("hardware profile: hardware.ram_bytes"));
        // Element size and every dimension count.
        assert_eq!(ids(&tiny, "src/a.c", "void f(void) { uint32_t m[4][8]; }"), ["EMB-HW-001"]);
        assert!(ids(&tiny, "src/a.c", "void f(void) { uint8_t m[4][8]; static uint8_t big[4096]; x = table[100]; }").is_empty());
        assert!(ids(&tiny, "src/a.c", "static uint8_t pool[4096];").is_empty());
        let fixed = prof("[architecture]\nmax_stack_object_bytes = 2048");
        assert!(ids(&fixed, "src/a.c", src).is_empty());
    }
    #[test]
    fn hw002_floating_point() {
        let src = "float scale(float x) { return x * 2.0f; }";
        let f = check(Pack::Hardware, &[("src/a.c", src)], &prof(TINY));
        assert!(f.iter().all(|f| f.rule.id == "EMB-HW-002") && !f.is_empty());
        assert!(render(&f, &prof(TINY)).1.contains("BARR-C:2018 5.4a"));
        let single = prof("[hardware]\nfpu = \"single\"");
        assert!(ids(&single, "src/a.c", src).is_empty());
        assert_eq!(ids(&single, "src/a.c", "double avg(void);"), ["EMB-HW-002"]);
    }
    #[test]
    fn hw003_flash_heavy_library() {
        let tiny = prof(TINY);
        assert!(ids(&tiny, "src/a.cpp", "#include <iostream>\n").contains(&"EMB-HW-003"));
        assert!(ids(&tiny, "src/a.c", "void f(int v) { log_write(\"v=%.2f\", v); }").contains(&"EMB-HW-003"));
        assert!(!ids(&tiny, "src/a.c", "void f(int v) { log_write(\"v=%d %5u 100%% %s\", v); }").contains(&"EMB-HW-003"));
    }
    #[test]
    fn hw004_exceptions_rtti() {
        let src = "void f() { try { g(); } catch (const E &) { throw; } auto *d = dynamic_cast<D *>(b); }";
        let no_heap = prof("[architecture]\nheap = \"none\"");
        let f = check(Pack::Hardware, &[("src/a.cpp", src)], &no_heap);
        assert_eq!(f.len(), 4);
        assert!(render(&f, &no_heap).1.contains("JSF AV Rule 208"));
        assert!(ids(&prof(TINY), "src/a.cpp", src).contains(&"EMB-HW-004"));
        // C has no exceptions: `try` is an ordinary name there.
        assert!(!ids(&no_heap, "src/a.c", "int try = 0;").contains(&"EMB-HW-004"));
    }
    #[test]
    fn hw005_execution_model() {
        let superloop = prof("[architecture]\nexecution_model = \"superloop\"\nheap = \"allowed\"\n[hardware]\nfpu = \"double\"");
        assert_eq!(ids(&superloop, "src/a.c", "void blink(void) { led_on(); HAL_Delay(500); }"), ["EMB-HW-005"]);
        assert_eq!(ids(&superloop, "src/a.c", "void start(void) { xTaskCreate(t, \"t\", 128, 0, 1, 0); vTaskDelay(10); }"), ["EMB-HW-005", "EMB-HW-005"]);
        assert!(ids(&superloop, "src/a.c", "void board_init(void) { HAL_Delay(10); }").is_empty());

        let rtos = prof("[architecture]\nexecution_model = \"rtos\"\nheap = \"allowed\"\n[hardware]\nfpu = \"double\"");
        assert_eq!(ids(&rtos, "src/a.c", "void sensor_task(void *a) { for (;;) { read(); HAL_Delay(100); } }"), ["EMB-HW-005"]);
        assert!(ids(&rtos, "src/a.c", "void sensor_task(void *a) { for (;;) { read(); vTaskDelay(100); } }").is_empty());

        let ev = prof("[architecture]\nexecution_model = \"event-driven\"\nheap = \"allowed\"\n[hardware]\nfpu = \"double\"");
        assert_eq!(ids(&ev, "src/a.c", "void on_button(const event_t *e) { k_sleep(K_MSEC(50)); }"), ["EMB-HW-005"]);
        assert_eq!(ids(&ev, "src/a.c", "void uart_rx_cb(void) { xQueueReceive(q, &b, portMAX_DELAY); }"), ["EMB-HW-005"]);
        assert_eq!(ids(&ev, "src/a.c", "void handle_tick(void) { while (1) { spin(); } }"), ["EMB-HW-005"]);
        assert!(ids(&ev, "src/a.c", "void on_button(const event_t *e) { post(EV_PRESS); } void worker(void) { k_sleep(K_MSEC(5)); }").is_empty());
    }
    #[test]
    fn hw006_profile_must_parse() {
        let p = Profile::default();
        let f = check(Pack::Hardware, &[("hardware-profile.toml", "[hardware]\nram = 4096\n")], &p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule.id, "EMB-HW-006");
        assert!(f[0].detail.contains("unknown key hardware.ram"));
        assert!(ids(&p, "hardware-profile.toml", "[hardware]\nram_bytes = 4096\n").is_empty());
        // Mandatory: no deviation.
        let dev = "# EMB-DEVIATION(EMB-HW-006): temporary key while migrating boards\n[hardware]\nram = 1\n";
        assert_eq!(ids(&p, "hardware-profile.toml", dev), ["EMB-HW-006"]);
    }
    #[test]
    fn hw_rules_follow_the_profile() {
        let src = "#include <iostream>\ndouble mean(void) { double acc[256]; try { fill(acc); } catch (...) { } return acc[0]; }";
        assert!(ids(&prof(TINY), "src/a.cpp", src).len() >= 4);
        assert_eq!(ids(&prof(BIG), "src/a.cpp", src), Vec::<&str>::new());
    }
    #[test]
    fn hw_scope() {
        let src = "float f(void) { uint8_t b[4096]; HAL_Delay(1); return 1.0f; }";
        let tiny = prof(TINY);
        assert!(!ids(&tiny, "src/a.c", src).is_empty());
        for p in ["tests/a_test.c", "third_party/a.c", "docs/a.md", "CMakeLists.txt"] {
            assert_eq!(ids(&tiny, p, src), Vec::<&str>::new(), "{p}");
        }
    }
    #[test]
    fn malformed_hw_input_never_panics() {
        for src in ["void f(void) { uint8_t a[", "void f(void) { int a[0x]; }", "\"%", "\"%5.", "void on_x(void) { while (", "#include"] {
            let _ = ids(&prof(TINY), "a.cpp", src);
        }
    }
}
