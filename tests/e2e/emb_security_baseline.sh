#!/usr/bin/env bash
# emb_security_baseline refuses security defects visible in a change.
source "$(dirname "$0")/lib.sh"
setup_repo emb_security_baseline
mkdir -p src keys config tests

printf 'static const char *wifi_password = "hunter2";\nconst char *pw(void)\n{\n    return wifi_password;\n}\n' > src/wifi.c
expect_refused "hard-coded password" src/wifi.c "EMB-SEC-001" "ETSI EN 303 645 Provision 5.1-1, 5.4-3" "CWE-798" "not deviatable"

printf '/* EMB-DEVIATION(EMB-SEC-001): factory default, changed on first boot */\nstatic const char *wifi_password = "hunter2";\nconst char *pw(void)\n{\n    return wifi_password;\n}\n' > src/wifi.c
expect_refused "a deviation cannot waive a mandatory rule" src/wifi.c "EMB-SEC-001"

printf -- '-----BEGIN EC %s\nMHcCAQEEIA==\n-----END EC %s\n' 'PRIVATE KEY-----' 'PRIVATE KEY-----' > keys/device.pem
expect_refused "private key in the repository" keys/device.pem "EMB-SEC-002" "Provision 5.4-1"

printf 'CONFIG_MQTT_PASSWORD="s3cret-value"\nCONFIG_ESP_TLS_INSECURE=y\n' > config/prj.conf
expect_refused "secret and insecure TLS in a config fragment" config/prj.conf "EMB-SEC-001"

printf '#include <string.h>\nvoid copy(char *dst, const char *src)\n{\n    strcpy(dst, src);\n}\n' > src/copy.c
expect_refused "strcpy" src/copy.c "EMB-SEC-006" "CWE-120, CWE-676"

printf '#include <string.h>\nvoid copy(char *dst, const char *src)\n{\n    /* EMB-DEVIATION(EMB-SEC-006): dst is sized by the caller contract, DR-21 */\n    strcpy(dst, src);\n}\n' > src/copy.c
expect_landed "a required rule can be deviated with a reason" src/copy.c

printf '#include <string.h>\n#include <stddef.h>\nvoid copy(char *dst, const char *src, size_t n)\n{\n    strncpy(dst, src, n);\n}\nconst char *endpoint(void)\n{\n    return "https://updates.vendor.io/fw.bin";\n}\n' > src/clean.c
expect_landed "bounded copy and an https endpoint" src/clean.c

printf 'static const char *password = "test-only";\nconst char *pw(void)\n{\n    return password;\n}\n' > tests/auth_test.c
expect_landed "test code is exempt" tests/auth_test.c
finish
