//! `emb_runtime_safety`: interrupt service routines, critical sections and
//! the watchdog.
//!
//! Clauses were checked against BARR-C:2018 section 6.5, NASA-GB-8719.13
//! section 4.6 and the CERT ids in MISRA C:2012 Addendum 3.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, Func, Lang, Pack, Rule};
use crate::profile::Exec;

macro_rules! rules {
    ($( $name:ident = $id:literal, $strict:literal, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Runtime, category: Category::Required, strict: $strict, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    ISR_BLOCKING = "EMB-RT-001", false, "an ISR shall not block, allocate, print or call the kernel's task-level API",
        "BARR-C:2018 6.5";
    IRQ_UNBALANCED = "EMB-RT-002", false, "a function that disables interrupts shall restore them",
        "BARR-C:2018 6.5";
    NON_REENTRANT = "EMB-RT-003", false, "non-reentrant library functions shall not be used with concurrent tasks",
        "SEI CERT C CON33-C; hardware profile: architecture.execution_model";
    WDT_DISABLED = "EMB-RT-004", true, "the watchdog shall not be disabled",
        "NASA-GB-8719.13 4.6";
    WDT_IN_ISR = "EMB-RT-005", true, "the watchdog shall not be fed from an interrupt",
        "NASA-GB-8719.13 4.6";
}

const BLOCKING: [&str; 30] = [
    "malloc", "calloc", "realloc", "free", "printf", "fprintf", "sprintf", "snprintf", "puts", "vprintf", "sleep", "usleep", "msleep",
    "delay", "HAL_Delay", "_delay_ms", "vTaskDelay", "vTaskDelayUntil", "k_sleep", "k_msleep", "osDelay", "pthread_mutex_lock",
    "k_mutex_lock", "osMutexAcquire", "mtx_lock", "k_sem_take", "pvPortMalloc", "vPortFree", "k_malloc", "k_free",
];
/// FreeRTOS task-level API families; the ISR form ends in `FromISR`.
const FREERTOS: [&str; 9] =
    ["xQueue", "xSemaphore", "xTaskNotify", "xTaskResume", "xTimer", "xEventGroup", "xStreamBuffer", "xMessageBuffer", "vTaskNotify"];

const IRQ_OFF: [&str; 11] = [
    "__disable_irq", "cli", "taskDISABLE_INTERRUPTS", "portDISABLE_INTERRUPTS", "noInterrupts", "irq_lock", "taskENTER_CRITICAL",
    "portENTER_CRITICAL", "local_irq_disable", "__disable_interrupt", "DisableInterrupts",
];
const IRQ_ON: [&str; 13] = [
    "__enable_irq", "sei", "taskENABLE_INTERRUPTS", "portENABLE_INTERRUPTS", "interrupts", "irq_unlock", "taskEXIT_CRITICAL",
    "portEXIT_CRITICAL", "local_irq_enable", "__enable_interrupt", "EnableInterrupts", "__set_PRIMASK", "SREG",
];
/// A function with one of these words in its name is the disabling half of a pair.
const IRQ_WRAPPER: [&str; 9] = ["disable", "enter", "lock", "critical", "panic", "fault", "halt", "reset", "mask"];

const NON_REENTRANT_FNS: [&str; 6] = ["strtok", "localtime", "gmtime", "asctime", "ctime", "strerror"];

const WDT_OFF: [&str; 10] = [
    "wdt_disable", "rtc_wdt_disable", "esp_task_wdt_deinit", "wdt_hal_disable", "WDOG_Disable", "WDT_Disable", "wdog_disable",
    "watchdog_disable", "IWDG_Disable", "esp_int_wdt_disable",
];
const WDT_FEED: [&str; 14] = [
    "wdt_reset", "wdt_feed", "wdt_kick", "WDT_Feed", "HAL_IWDG_Refresh", "HAL_WWDG_Refresh", "IWDG_ReloadCounter",
    "esp_task_wdt_reset", "watchdog_feed", "watchdog_kick", "wdog_feed", "kick_watchdog", "feed_watchdog", "task_wdt_feed",
];
/// Config keys that must not be `n`, and keys that must not be `y`.
const WDT_KEEP_ON: [&str; 5] = ["CONFIG_WATCHDOG", "CONFIG_ESP_TASK_WDT", "CONFIG_ESP_TASK_WDT_EN", "CONFIG_ESP_INT_WDT", "CONFIG_BOOTLOADER_WDT_ENABLE"];
const WDT_KEEP_OFF: [&str; 2] = ["CONFIG_WDT_DISABLE_AT_BOOT", "CONFIG_BOOTLOADER_WDT_DISABLE_IN_USER_CODE"];

/// `KEY=value` and `# KEY is not set` lines of a Kconfig fragment.
pub(crate) fn kconfig(text: &str) -> impl Iterator<Item = (u32, &str, &str)> {
    text.lines().enumerate().filter_map(|(n, raw)| {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix('#') {
            let key = rest.trim().strip_suffix("is not set")?.trim();
            return key.starts_with("CONFIG_").then_some((n as u32 + 1, key, "n"));
        }
        let (k, v) = line.split_once('=')?;
        Some((n as u32 + 1, k.trim(), v.trim().trim_matches('"')))
    })
}

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if !cx.is_production() {
        return;
    }
    match cx.lang {
        Lang::C | Lang::Cpp => code(s),
        Lang::Config | Lang::Build => {
            for (line, key, value) in kconfig(cx.text) {
                if (WDT_KEEP_ON.contains(&key) && value == "n") || (WDT_KEEP_OFF.contains(&key) && value == "y") {
                    s.hit(&WDT_DISABLED, line, format!("{key}={value} disables the watchdog"));
                }
            }
        }
        _ => {}
    }
}

fn code(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let concurrent = cx.profile.exec != Exec::Superloop;
    for (i, tok) in t.iter().enumerate() {
        if tok.pp || tok.kind != Kind::Ident || !cx.is_call(i) {
            continue;
        }
        let name = tok.text;
        if WDT_OFF.contains(&name) {
            s.hit(&WDT_DISABLED, tok.line, format!("{name}() disables the watchdog"));
        }
        if concurrent && NON_REENTRANT_FNS.contains(&name) {
            s.hit(&NON_REENTRANT, tok.line, format!("{name}() keeps hidden static state shared by every task"));
        }
    }
    for f in &cx.funcs {
        if f.is_isr {
            isr(s, f);
        }
        irq_balance(s, f);
    }
}

fn isr(s: &mut Sink, f: &Func) {
    let cx = s.cx;
    let t = cx.toks();
    for k in f.open + 1..f.close {
        if t[k].pp || t[k].kind != Kind::Ident || !cx.is_call(k) {
            continue;
        }
        let name = t[k].text;
        if WDT_FEED.contains(&name) {
            s.hit(&WDT_IN_ISR, t[k].line, format!("{name}() in ISR {}(): an interrupt can keep firing while every task is stuck", f.name));
        } else if BLOCKING.contains(&name) {
            s.hit(&ISR_BLOCKING, t[k].line, format!("{name}() in ISR {}()", f.name));
        } else if FREERTOS.iter().any(|p| name.starts_with(p)) && !name.contains("FromISR") {
            s.hit(&ISR_BLOCKING, t[k].line, format!("{name}() in ISR {}(): use the FromISR variant", f.name));
        }
    }
}

fn irq_balance(s: &mut Sink, f: &Func) {
    let cx = s.cx;
    let t = cx.toks();
    let lname = f.name.to_ascii_lowercase();
    if IRQ_WRAPPER.iter().any(|w| lname.contains(w)) {
        return;
    }
    let mut first_off = None;
    for k in f.open + 1..f.close {
        if t[k].pp || t[k].kind != Kind::Ident {
            continue;
        }
        if IRQ_ON.contains(&t[k].text) {
            return;
        }
        if first_off.is_none() && IRQ_OFF.contains(&t[k].text) {
            first_off = Some(k);
        }
    }
    if let Some(k) = first_off {
        s.hit(&IRQ_UNBALANCED, t[k].line, format!("{}() calls {} and never restores interrupts", f.name, t[k].text));
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn rtos() -> Profile {
        Profile::parse("[architecture]\nexecution_model = \"rtos\"").unwrap()
    }
    fn ids_with(p: &Profile, path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Runtime, &[(path, src)], p).iter().map(|f| f.rule.id).collect()
    }
    fn ids(src: &str) -> Vec<&'static str> {
        ids_with(&rtos(), "src/drv.c", src)
    }
    fn rule(id: &str, clause: &str, bad: &str, good: &str) {
        let p = rtos();
        let f = check(Pack::Runtime, &[("src/drv.c", bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids(bad));
        assert!(render(&f, &p).1.contains(clause));
        assert!(!ids(good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn rt001_blocking_call_in_isr() {
        rule(
            "EMB-RT-001",
            "BARR-C:2018 6.5",
            "void USART1_IRQHandler(void) { printf(\"rx\\n\"); }",
            "void USART1_IRQHandler(void) { rx_flag = 1U; } void logger_task(void) { printf(\"rx\\n\"); }",
        );
        for bad in [
            "void uart_isr(void) { xQueueSend(q, &b, 0); }",
            "void SysTick_Handler(void) { HAL_Delay(1); }",
            "ISR(TIMER1_COMPA_vect) { buf = malloc(8); }",
            "void __interrupt timer0(void) { k_mutex_lock(&m, K_FOREVER); }",
            "void IRAM_ATTR gpio_edge(void *arg) { vTaskDelay(1); }",
        ] {
            assert!(ids(bad).contains(&"EMB-RT-001"), "{bad}");
        }
    }
    #[test]
    fn rt002_interrupts_left_disabled() {
        rule(
            "EMB-RT-002",
            "BARR-C:2018 6.5",
            "void update(void) { __disable_irq(); shared++; }",
            "void update(void) { __disable_irq(); shared++; __enable_irq(); }",
        );
        assert!(ids("void put(int v) { taskENTER_CRITICAL(); q[n++] = v; }").contains(&"EMB-RT-002"));
    }
    #[test]
    fn rt003_non_reentrant_library() {
        rule("EMB-RT-003", "SEI CERT C CON33-C", "char *next(char *s) { return strtok(s, \",\"); }", "char *next(char *s, char **save) { return strtok_r(s, \",\", save); }");
        // One thread of control: nothing to race with.
        let superloop = Profile::parse("[architecture]\nexecution_model = \"superloop\"").unwrap();
        assert!(ids_with(&superloop, "src/a.c", "char *next(char *s) { return strtok(s, \",\"); }").is_empty());
    }
    #[test]
    fn rt004_watchdog_disabled() {
        rule("EMB-RT-004", "NASA-GB-8719.13 4.6", "void boot(void) { wdt_disable(); }", "void boot(void) { wdt_enable(WDTO_2S); }");
        let p = rtos();
        assert_eq!(ids_with(&p, "prj.conf", "CONFIG_WATCHDOG=n\n"), ["EMB-RT-004"]);
        assert_eq!(ids_with(&p, "sdkconfig.defaults", "# CONFIG_ESP_TASK_WDT is not set\nCONFIG_WDT_DISABLE_AT_BOOT=y\n"), ["EMB-RT-004", "EMB-RT-004"]);
        assert!(ids_with(&p, "prj.conf", "CONFIG_WATCHDOG=y\n# CONFIG_FOO is not set\nCONFIG_WDT_DISABLE_AT_BOOT=n\n").is_empty());
    }
    #[test]
    fn rt005_watchdog_fed_from_isr() {
        rule(
            "EMB-RT-005",
            "NASA-GB-8719.13 4.6",
            "void TIM2_IRQHandler(void) { HAL_IWDG_Refresh(&hiwdg); }",
            "void health_task(void *a) { for (;;) { if (all_alive()) { HAL_IWDG_Refresh(&hiwdg); } } }",
        );
    }

    #[test]
    fn rt_does_not_flag_correct_code() {
        let src = r#"
void USART1_IRQHandler(void)
{
    BaseType_t woken = pdFALSE;
    xQueueSendFromISR(rx_q, &byte, &woken);
    xSemaphoreGiveFromISR(sem, &woken);
    portYIELD_FROM_ISR(woken);
}
void counter_add(uint32_t n)
{
    uint32_t key = irq_lock();
    total += n;
    irq_unlock(key);
}
void port_enter_critical(void) { __disable_irq(); nesting++; }
void fault_halt(void) { __disable_irq(); for (;;) { } }
void avr_update(void) { uint8_t s = SREG; cli(); shared++; SREG = s; }
void supervisor_task(void *arg) { for (;;) { wdt_feed(wdt, ch); k_sleep(K_MSEC(100)); } }
"#;
        assert_eq!(ids(src), Vec::<&str>::new());
    }
    #[test]
    fn rt_scope() {
        let bad = "void uart_isr(void) { printf(\"x\"); wdt_disable(); }";
        assert_eq!(ids(bad).len(), 2);
        for p in ["tests/isr_test.c", "third_party/hal/x.c", "docs/x.md"] {
            assert_eq!(ids_with(&rtos(), p, bad), Vec::<&str>::new(), "{p}");
        }
        assert!(ids_with(&rtos(), "tests/prj.conf", "CONFIG_WATCHDOG=n\n").is_empty());
    }
    #[test]
    fn rt_watchdog_strict_on_high_integrity() {
        let src = "void boot(void) {\n  /* EMB-DEVIATION(EMB-RT-004): disabled during flash erase, DR-9 */\n  wdt_disable();\n}";
        assert!(ids(src).is_empty());
        let sil3 = Profile::parse("[safety]\niec61508_sil = 3").unwrap();
        assert_eq!(ids_with(&sil3, "src/a.c", src), ["EMB-RT-004"]);
    }
    #[test]
    fn malformed_rt_input_never_panics() {
        for src in ["void x_isr(void) {", "ISR(", "void f(void) { cli(", "CONFIG_=", "# is not set", "=\n#\n"] {
            let _ = ids(src);
            let _ = ids_with(&rtos(), "a.conf", src);
        }
    }
}
