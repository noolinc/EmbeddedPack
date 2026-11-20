#!/usr/bin/env bash
# emb_runtime_safety refuses ISR, critical-section and watchdog defects.
# The repository profile is an RTOS target.
source "$(dirname "$0")/lib.sh"
setup_repo emb_runtime_safety
mkdir -p src config tests

printf 'int printf(const char *fmt, ...);\nvoid USART1_IRQHandler(void)\n{\n    printf("rx");\n}\n' > src/isr.c
expect_refused "printf in an ISR" src/isr.c "EMB-RT-001" "BARR-C:2018 6.5" "src/isr.c:4"

printf 'void __disable_irq(void);\nstatic unsigned shared;\nvoid bump(void)\n{\n    __disable_irq();\n    shared++;\n}\n' > src/crit.c
expect_refused "interrupts disabled and never restored" src/crit.c "EMB-RT-002" "never restores"

printf 'void wdt_disable(void);\nvoid boot(void)\n{\n    wdt_disable();\n}\n' > src/wdt.c
expect_refused "watchdog disabled in code" src/wdt.c "EMB-RT-004" "NASA-GB-8719.13 4.6"

printf 'CONFIG_WATCHDOG=n\n' > config/prj.conf
expect_refused "watchdog disabled in config" config/prj.conf "EMB-RT-004" "CONFIG_WATCHDOG=n"

printf 'void wdt_feed(void);\nvoid TIM2_IRQHandler(void)\n{\n    wdt_feed();\n}\n' > src/feed.c
expect_refused "watchdog fed from an ISR" src/feed.c "EMB-RT-005"

printf 'void __disable_irq(void);\nvoid __enable_irq(void);\nstatic volatile unsigned rx_flag;\nstatic unsigned shared;\nvoid USART1_IRQHandler(void)\n{\n    rx_flag = 1U;\n}\nvoid bump(void)\n{\n    __disable_irq();\n    shared++;\n    __enable_irq();\n}\n' > src/good.c
expect_landed "flag-only ISR and a balanced critical section" src/good.c

printf 'CONFIG_WATCHDOG=y\n' > config/prj.conf
expect_landed "watchdog enabled in config" config/prj.conf

printf 'void wdt_disable(void);\nvoid setup(void)\n{\n    wdt_disable();\n}\n' > tests/wdt_test.c
expect_landed "test code is exempt" tests/wdt_test.c
finish
