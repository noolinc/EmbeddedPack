cmake_minimum_required(VERSION 3.21)
project(embedded_regressions LANGUAGES C)

enable_testing()

foreach(scenario IN ITEMS priority_inversion counter_rollover uart_tx)
    add_executable(${scenario} ${scenario}.c)
    target_compile_features(${scenario} PRIVATE c_std_11)
    add_test(NAME ${scenario} COMMAND ${scenario})
    # Each test runs in well under a second and fails by assertion, never by
    # hanging. The timeout only guards against a stuck process; it is generous
    # because a freshly built binary can take seconds to start on a busy host.
    set_tests_properties(${scenario} PROPERTIES TIMEOUT 60)
endforeach()
