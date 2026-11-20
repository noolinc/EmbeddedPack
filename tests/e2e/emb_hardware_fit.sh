#!/usr/bin/env bash
# emb_hardware_fit refuses code that does not fit the profile it was built
# with. The repository profile: 64 KiB RAM (stack objects up to 1024 bytes),
# single-precision FPU, RTOS, no heap.
source "$(dirname "$0")/lib.sh"
setup_repo emb_hardware_fit
mkdir -p src tests

printf '#include <stdint.h>\nuint8_t first(void)\n{\n    uint8_t frame[4096];\n    frame[0] = 1U;\n    return frame[0];\n}\n' > src/stack.c
expect_refused "4 KiB array on the stack" src/stack.c "EMB-HW-001" "4096 bytes" "allows 1024" "generic-cortex-m4"

printf 'double half(double x)\n{\n    return x / 2.0;\n}\n' > src/dbl.c
expect_refused "double on a single-precision FPU" src/dbl.c "EMB-HW-002" "single-precision"

printf 'void HAL_Delay(unsigned ms);\nvoid sensor_task(void *arg)\n{\n    (void)arg;\n    for (;;)\n    {\n        HAL_Delay(100U);\n    }\n}\n' > src/task.c
expect_refused "busy-wait delay under an RTOS" src/task.c "EMB-HW-005" "busy-waits"

printf 'void g();\nvoid f()\n{\n    try {\n        g();\n    } catch (...) {\n    }\n}\n' > src/exc.cpp
expect_refused "exceptions without a heap" src/exc.cpp "EMB-HW-004" "JSF AV Rule 208"

printf '[hardware]\nram = 4096\n' > hardware-profile.toml
expect_refused "profile with an unknown key" hardware-profile.toml "EMB-HW-006" "unknown key hardware.ram"
rm hardware-profile.toml

printf '#include <stdint.h>\nfloat scale(float x)\n{\n    uint8_t scratch[64];\n    scratch[0] = 2U;\n    return x * (float)scratch[0];\n}\n' > src/fits.c
expect_landed "float and a 64-byte buffer fit this target" src/fits.c

printf 'double big(void)\n{\n    char buf[8192];\n    buf[0] = 0;\n    return (double)buf[0];\n}\n' > tests/big_test.c
expect_landed "test code is exempt" tests/big_test.c
finish
