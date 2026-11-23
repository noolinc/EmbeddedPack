cmake_minimum_required(VERSION 3.21)
project(embedded_regressions LANGUAGES C)

enable_testing()

foreach(scenario IN ITEMS priority_inversion counter_rollover uart_tx)
    add_executable(${scenario} ${scenario}.c)
    target_compile_features(${scenario} PRIVATE c_std_11)
    add_test(NAME ${scenario} COMMAND ${scenario})
    set_tests_properties(${scenario} PROPERTIES TIMEOUT 5)
endforeach()
