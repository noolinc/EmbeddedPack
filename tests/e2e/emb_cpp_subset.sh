#!/usr/bin/env bash
# emb_cpp_subset blocks a real `nool propose` that violates the C++ subset.
source "$(dirname "$0")/lib.sh"
setup_repo emb_cpp_subset
mkdir -p src inc tests

printf '#include <cstdint>\nstd::uint8_t low(std::uint32_t v)\n{\n    return (std::uint8_t)v;\n}\n' > src/cast.cpp
expect_refused "C-style cast" src/cast.cpp "EMB-CPP-001" "AUTOSAR C++14 A5-2-2" "JSF AV Rule 185" "src/cast.cpp:4"

printf '#include <cstdio>\nint *find()\n{\n    return NULL;\n}\n' > src/null.cpp
expect_refused "cstdio and NULL" src/null.cpp "EMB-CPP-006" "M27-0-1"

printf '#pragma once\nusing namespace std;\nenum Color { Red, Green };\n' > inc/color.hpp
expect_refused "using namespace + unscoped enum in header" inc/color.hpp "EMB-CPP-010" "AUTOSAR C++14 M7-3-6"

printf '#include <cstdint>\nenum class Mode : std::uint8_t { Idle, Run };\nstd::uint32_t step(Mode m, std::uint32_t rpm)\n{\n    std::uint32_t out = 0U;\n    switch (m) {\n        case Mode::Run:\n            out = static_cast<std::uint32_t>(rpm);\n            break;\n        default:\n            break;\n    }\n    return out;\n}\n' > src/clean.cpp
expect_landed "clean C++ file" src/clean.cpp

printf '#include <cstdio>\nint main()\n{\n    int *p = NULL;\n    return (int)(p == NULL);\n}\n' > tests/unit_test.cpp
expect_landed "test code is exempt" tests/unit_test.cpp

# C files are left to emb_c_subset.
printf 'int *find(void)\n{\n    return (int *)0;\n}\n' > src/plain.c
expect_landed "C file is not this policy's" src/plain.c
finish
