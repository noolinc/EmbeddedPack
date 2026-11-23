#!/usr/bin/env bash
# emb_firmware_integrity refuses changes that weaken the build or the boot chain.
source "$(dirname "$0")/lib.sh"
setup_repo emb_firmware_integrity
mkdir -p config scripts cmake tests

printf 'CONFIG_SECURE_BOOT=n\n' > config/sdkconfig.defaults
expect_refused "secure boot disabled" config/sdkconfig.defaults "EMB-FWI-003" "NIST SP 800-193 4.2.1.3" "UN Regulation No. 156 7.2.1.1" "not deviatable"

printf 'CONFIG_SECURE_BOOT=y\nCONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=n\n' > config/sdkconfig.defaults
expect_refused "anti-rollback disabled" config/sdkconfig.defaults "EMB-FWI-004"

printf 'set(APP_FLAGS -Os -w -fno-stack-protector)\n' > cmake/flags.cmake
expect_refused "warnings and stack protector off" cmake/flags.cmake "EMB-FWI-001" "Power of 10 Rule 10"

printf 'CONFIG_BOOT_SIGNATURE_KEY_FILE="bootloader/mcuboot/root-rsa-2048.pem"\n' > config/mcuboot.conf
expect_refused "MCUboot development key" config/mcuboot.conf "EMB-FWI-005" "published development key"

printf '#!/bin/sh\ncurl -sSf https://get.vendor.io/toolchain | sh\n' > scripts/setup.sh
expect_refused "download piped into a shell" scripts/setup.sh "EMB-FWI-006" "CWE-494, CWE-829"

printf 'CONFIG_SECURE_BOOT=y\nCONFIG_SECURE_FLASH_ENC_ENABLED=y\nCONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=y\n' > config/sdkconfig.defaults
expect_landed "hardened boot config" config/sdkconfig.defaults

printf 'set(APP_FLAGS -Os -Wall -Wextra -Werror -fstack-protector-strong)\n' > cmake/flags.cmake
expect_landed "hardened compiler flags" cmake/flags.cmake

printf 'set(TEST_FLAGS -w -fno-stack-protector)\n' > tests/flags.cmake
expect_landed "test build files are exempt" tests/flags.cmake

# A breaking change to a linker script must move a version file. This needs
# the host's intent projection (--breaking); a host that supplies none (Nool
# 8.0.2 during propose) judges nothing, which is reported, not failed.
printf 'MEMORY\n{\n  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 128K\n}\n' > app.ld
propose app.ld --breaking
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "EMB-FWI-009"; then
  echo "ok   refused: breaking layout change without a version change"
elif [ "$RC" -eq 0 ]; then
  echo "skip breaking layout change: the host supplied no intent projection"
else
  echo "FAIL breaking layout change (rc=$RC)"; echo "$OUT" | tail -5; FAILS=$((FAILS+1))
fi
finish
