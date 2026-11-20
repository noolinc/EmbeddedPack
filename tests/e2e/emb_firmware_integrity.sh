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
finish
