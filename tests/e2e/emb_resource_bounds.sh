#!/usr/bin/env bash
# emb_resource_bounds blocks heap use, recursion and unbounded stack or loops.
# The profile it is built with (hardware-profile.toml) has heap = "none".
source "$(dirname "$0")/lib.sh"
setup_repo emb_resource_bounds
mkdir -p src tests

# Fixtures must compile: Nool builds each proposed file before the gate's verdict.
printf '#include <stdlib.h>\nchar *rx(void)\n{\n    char *p = malloc(32);\n    return p;\n}\n' > src/heap.c
expect_refused "malloc with heap = none" src/heap.c "EMB-RES-001" "MISRA C:2012 Dir 4.12, Rule 21.3" "Power of 10 Rule 3" "src/heap.c:4"

printf 'struct node { struct node *next; };\nint depth(const struct node *n)\n{\n    return (n == 0) ? 0 : 1 + depth(n->next);\n}\n' > src/rec.c
expect_refused "recursion" src/rec.c "EMB-RES-002" "MISRA C:2012 Rule 17.2" "JSF AV Rule 119"

printf 'unsigned char fill(unsigned n)\n{\n    unsigned char buf[n];\n    buf[0] = 1U;\n    return buf[0];\n}\n' > src/vla.c
expect_refused "variable-length array" src/vla.c "EMB-RES-003" "MISRA C:2012 Rule 18.8"

printf 'struct Node { Node *next; };\nclass Q {\npublic:\n    void push() { head_ = new Node(); }\nprivate:\n    Node *head_;\n};\n' > src/q.cpp
expect_refused "new expression in C++" src/q.cpp "EMB-RES-001" "AUTOSAR C++14 A18-5-1, A18-5-2"

printf '#include <stdint.h>\nstatic uint8_t pool[64];\nint main(void)\n{\n    for (;;)\n    {\n        pool[0]++;\n    }\n}\n' > src/main.c
expect_landed "static pool and the main loop" src/main.c

printf '#include <stdlib.h>\nint main(void)\n{\n    char *p = malloc(8);\n    free(p);\n    return 0;\n}\n' > tests/heap_test.c
expect_landed "test code is exempt" tests/heap_test.c
finish
