#!/usr/bin/env bash
# emb_c_subset blocks a real `nool propose` that violates the C subset.
source "$(dirname "$0")/lib.sh"
setup_repo emb_c_subset
mkdir -p src tests

printf 'int parse(const char *s)\n{\n    if (s == 0)\n    {\n        goto fail;\n    }\n    return 1;\nfail:\n    return 0;\n}\n' > src/goto.c
expect_refused "goto (single file)" src/goto.c "EMB-C-001" "MISRA C:2012 Rule 15.1" "src/goto.c:5"

printf '#include <stdio.h>\nint mode(int x)\n{\n    switch (x)\n    {\n        case 1:\n            return 010;\n    }\n    return 0;\n}\n' > src/multi.c
expect_refused "stdio, octal, switch default" src/multi.c "EMB-C-007" "MISRA C:2012 Rule 21.6"

printf '#include <stdint.h>\nuint32_t twice(uint32_t x)\n{\n    return x * 2U;\n}\n' > src/clean.c
expect_landed "clean C file" src/clean.c

# A change to a landed file is judged on its new content.
printf '#include <stdint.h>\nuint32_t twice(uint32_t x)\n{\n    if (x > 4U) return 8U;\n    return x * 2U;\n}\n' > src/clean.c
expect_refused "edit introduces an unbraced if" src/clean.c "EMB-C-013" "BARR-C:2018 1.3a"

# Several files in one proposal: one bad file refuses the whole change.
printf '#include <stdint.h>\nuint32_t twice(uint32_t x)\n{\n    return x * 2U;\n}\n' > src/clean.c
printf 'int f(const char *s)\n{\n    return atoi(s);\n}\n' > src/conv.c
printf '#include <stdint.h>\nuint32_t one(void)\n{\n    return 1U;\n}\n' > src/one.c
OUT="$(nool propose --path src/one.c --path src/conv.c --intent "e2e multi" --why "policy end-to-end test" --solidify 2>&1)"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "EMB-C-003"; then echo "ok   refused: multi-file change"; else echo "FAIL multi-file"; echo "$OUT" | tail -5; FAILS=$((FAILS+1)); fi
rm src/conv.c

# Test code is outside the subset.
printf '#include <stdio.h>\nint main(void)\n{\n    printf("ok");\n    return 0;\n}\n' > tests/unit.c
expect_landed "test code is exempt" tests/unit.c

# A justified deviation lets a required rule through.
printf 'int parse(const char *s)\n{\n    if (s == 0)\n    {\n        /* EMB-DEVIATION(EMB-C-001): single cleanup exit, approved in DR-7 */\n        goto fail;\n    }\n    return 1;\nfail:\n    return 0;\n}\n' > src/goto.c
expect_landed "justified deviation" src/goto.c

# MISRA C:2012 Rule 1.4 (after AMD3): _Atomic needs a deviation on any profile.
printf '_Atomic int ticks;\n' > src/ticks.c
expect_refused "restricted C11 feature" src/ticks.c "EMB-C-021" "AMD2 Rule 1.4"

# A C99 target: newer constructs and a build that selects C11 are refused.
# Nool's own syntax check accepts both, so the refusal is this policy's.
setup_repo emb_c_subset '[language]
c_standard = "c99"'
mkdir -p src
printf '_Static_assert(sizeof(int) == 4, "int is 32 bits");\nint f(void)\n{\n    return 0;\n}\n' > src/s.c
expect_refused "C11 construct on a C99 target" src/s.c "EMB-C-020" "is C11; this target is built as C99"
printf 'cmake_minimum_required(VERSION 3.20)\nproject(fw C)\nset(CMAKE_C_STANDARD 11)\n' > CMakeLists.txt
expect_refused "build selects C11 on a C99 target" CMakeLists.txt "EMB-C-022" "builds as C11; [language] says C99"
printf 'cmake_minimum_required(VERSION 3.20)\nproject(fw C)\nset(CMAKE_C_STANDARD 99)\n' > CMakeLists.txt
expect_landed "build selects the profile's edition" CMakeLists.txt
printf '#include <stdint.h>\nstatic inline uint32_t twice(uint32_t v)\n{\n    return v * 2U;\n}\n' > src/s.c
expect_landed "C99 code on a C99 target" src/s.c
finish
