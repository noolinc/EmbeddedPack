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
finish
