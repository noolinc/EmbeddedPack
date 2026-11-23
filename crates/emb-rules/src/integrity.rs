//! `emb_firmware_integrity`: the build and boot configuration that keeps
//! firmware trustworthy — compiler diagnostics and hardening, secure boot,
//! update authenticity, rollback protection, signing keys, debug access.
//!
//! Clauses were checked against NIST SP 800-193 (4.2.1.3), NIST SP 800-218
//! (PW.4.1, PW.6.1, PW.6.2), UN Regulation No. 156 (7.1.1.7, 7.2.1.1), IETF
//! RFC 9124 (4.3.2), ETSI EN 303 645 V3.1.3, the CWE-699 data set and the
//! published Power of 10 rules.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, ChangeInfo, Finding, Lang, Pack, Rule, file_name, is_linker_script, is_partition_table, is_test_path, is_third_party};
use crate::profile::{Profile, any_glob};
use crate::runtime::kconfig;

macro_rules! rules {
    ($( $name:ident = $id:literal, $cat:ident, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Integrity, category: Category::$cat, strict: true, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    WARNINGS = "EMB-FWI-001", Required, "compiler diagnostics shall not be silenced or demoted",
        "Power of 10 Rule 10; CWE-1127; NIST SP 800-218 PW.6.2";
    HARDENING = "EMB-FWI-002", Required, "exploit mitigations shall not be turned off",
        "NIST SP 800-218 PW.6.1, PW.6.2";
    BOOT_VERIFY = "EMB-FWI-003", Mandatory, "secure boot, signature verification and flash encryption shall stay enabled",
        "ETSI EN 303 645 Provision 5.7-1, 5.3-10; NIST SP 800-193 4.2.1.3; UN Regulation No. 156 7.2.1.1; CWE-494";
    ROLLBACK = "EMB-FWI-004", Required, "rollback protection shall stay enabled",
        "NIST SP 800-193 4.2.1.3";
    DEV_KEY = "EMB-FWI-005", Mandatory, "published development keys shall not sign firmware",
        "ETSI EN 303 645 Provision 5.4-3; CWE-798";
    FETCH = "EMB-FWI-006", Required, "the build shall not run or fetch unauthenticated downloads",
        "CWE-494, CWE-829; NIST SP 800-218 PW.4.1";
    DEBUG_OPEN = "EMB-FWI-007", Required, "debug and readout access shall be closed",
        "NIST SP 800-193 4.2.1.3; ETSI EN 303 645 Provision 5.6-3; CWE-489";
    LAYOUT_UNDECLARED = "EMB-FWI-008", Advisory, "a change to a boot or persisted layout shall be declared breaking or confirmed compatible",
        "IETF RFC 9124 4.3.2 REQ.SEC.COMPATIBLE; UN Regulation No. 156 7.1.1.7";
    LAYOUT_VERSION = "EMB-FWI-009", Required, "a breaking layout change shall change the firmware or schema version",
        "IETF RFC 9124 4.3.2 REQ.SEC.COMPATIBLE; UN Regulation No. 156 7.1.1.7";
}

/// Why a file's layout outlives an update, if it does.
pub fn layout_file(path: &str, profile: &Profile) -> Option<&'static str> {
    if is_test_path(path) || is_third_party(path) {
        return None;
    }
    if is_linker_script(path) {
        Some("linker script")
    } else if is_partition_table(path) {
        Some("flash partition table")
    } else if any_glob(&profile.layout_paths, path) {
        Some("persisted layout ([compatibility] layout_paths)")
    } else {
        None
    }
}

/// Files that carry the firmware or schema version.
pub fn version_file(path: &str, profile: &Profile) -> bool {
    let n = file_name(path).to_ascii_lowercase();
    matches!(n.as_str(), "version" | "version.txt" | "version.h" | "version.cmake" | "version.json" | "app_version.h")
        || n.ends_with("_version.h")
        || any_glob(&profile.version_paths, path)
}

/// Warnings whose loss hides undefined behaviour or memory errors.
const CRITICAL_WARNINGS: [&str; 16] = [
    "implicit-function-declaration", "return-type", "uninitialized", "maybe-uninitialized", "format", "format-security",
    "array-bounds", "stringop-overflow", "incompatible-pointer-types", "int-conversion", "strict-aliasing", "shift-overflow",
    "overflow", "div-by-zero", "implicit-int", "all",
];
const NO_HARDENING: [&str; 6] =
    ["-fno-stack-protector", "-fno-stack-clash-protection", "-U_FORTIFY_SOURCE", "-fcf-protection=none", "-mbranch-protection=none", "-fno-sanitize=all"];

const BOOT_MUST_NOT_BE_Y: [&str; 7] = [
    "CONFIG_SECURE_BOOT_INSECURE", "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT", "CONFIG_SECURE_FLASH_UART_BOOTLOADER_ALLOW_ENC",
    "CONFIG_SECURE_FLASH_UART_BOOTLOADER_ALLOW_DEC", "CONFIG_SECURE_FLASH_UART_BOOTLOADER_ALLOW_CACHE",
    "CONFIG_BOOT_SIGNATURE_TYPE_NONE", "CONFIG_SECURE_BOOT_ALLOW_ROM_BASIC",
];
const BOOT_MUST_NOT_BE_N: [&str; 4] =
    ["CONFIG_SECURE_BOOT", "CONFIG_SECURE_FLASH_ENC_ENABLED", "CONFIG_BOOT_VALIDATE_SLOT0", "CONFIG_SECURE_SIGNED_APPS"];
const ROLLBACK_MUST_NOT_BE_N: [&str; 3] =
    ["CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK", "CONFIG_MCUBOOT_DOWNGRADE_PREVENTION", "CONFIG_MCUBOOT_HW_DOWNGRADE_PREVENTION"];
const DEBUG_MUST_NOT_BE_Y: [&str; 1] = ["CONFIG_SECURE_BOOT_ALLOW_JTAG"];

/// The keys MCUboot ships in its repository; their private halves are public.
const DEV_KEYS: [&str; 8] = [
    "root-rsa-2048.pem", "root-rsa-3072.pem", "root-ec-p256.pem", "root-ec-p384.pem", "root-ed25519.pem", "enc-rsa2048-priv.pem",
    "enc-ec256-priv.pem", "enc-x25519-priv.pem",
];
const FETCHERS: [&str; 8] = ["curl", "wget", "git clone", "FetchContent", "ExternalProject", "pip install", "URL ", "submodule"];
const LOCAL: [&str; 3] = ["http://localhost", "http://127.", "http://[::1]"];

/// Rules over the change as a whole: a layout change must say whether it
/// breaks devices in the field, and a breaking one must move the version.
pub fn run_change(files: &[(&str, &str)], profile: &Profile, change: &ChangeInfo) -> Vec<Finding> {
    let mut out = Vec::new();
    if !change.known {
        return out;
    }
    let Some((path, kind)) = files.iter().find_map(|(p, _)| layout_file(p, profile).map(|k| (*p, k))) else {
        return out;
    };
    if !change.breaking {
        out.push(Finding {
            rule: &LAYOUT_UNDECLARED,
            path: path.to_string(),
            line: 0,
            detail: format!("this {kind} decides where devices in the field find code and data; confirm the layout is unchanged, or propose again with --breaking and a version change"),
        });
    } else if !files.iter().any(|(p, _)| version_file(p, profile)) {
        out.push(Finding {
            rule: &LAYOUT_VERSION,
            path: path.to_string(),
            line: 0,
            detail: format!("the change is marked --breaking and changes this {kind}, but no version file (VERSION, *_version.h, [compatibility] version_paths) changes with it"),
        });
    }
    out
}

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if !cx.is_production() {
        return;
    }
    match cx.lang {
        Lang::Build | Lang::Config | Lang::Script => {
            lines(s);
            config_keys(s);
        }
        Lang::C | Lang::Cpp => {
            for tok in cx.toks() {
                if tok.kind == Kind::Ident && tok.text == "OB_RDP_LEVEL_0" {
                    s.hit(&DEBUG_OPEN, tok.line, "OB_RDP_LEVEL_0 removes flash readout protection");
                }
                if tok.kind == Kind::Str {
                    if let Some(k) = DEV_KEYS.iter().find(|k| tok.text.contains(**k)) {
                        s.hit(&DEV_KEY, tok.line, format!("{k} is a published development key"));
                    }
                }
            }
        }
        Lang::Other => {}
    }
}

fn is_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//") || line.starts_with(';')
}

fn lines(s: &mut Sink) {
    let cx = s.cx;
    for (n, raw) in cx.text.lines().enumerate() {
        let line = raw.trim();
        let at = n as u32 + 1;
        if line.is_empty() || is_comment(line) {
            continue;
        }
        if let Some(k) = DEV_KEYS.iter().find(|k| line.contains(**k)) {
            s.hit(&DEV_KEY, at, format!("{k} is a published development key"));
        }
        // Compiler and linker flags, wherever the build system puts them.
        for word in line.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\'' | ';' | '[' | ']')) {
            let w = word.trim_end_matches(',');
            if w == "-w" || w.starts_with("-Wno-error") || w == "-fpermissive" {
                s.hit(&WARNINGS, at, format!("{w} silences or demotes diagnostics"));
            } else if let Some(name) = w.strip_prefix("-Wno-") {
                if CRITICAL_WARNINGS.contains(&name) {
                    s.hit(&WARNINGS, at, format!("{w} hides a safety-relevant warning"));
                }
            } else if NO_HARDENING.contains(&w) || w.contains("_FORTIFY_SOURCE=0") || w.contains("execstack") || w.contains("norelro") {
                s.hit(&HARDENING, at, format!("{w} removes an exploit mitigation"));
            }
        }
        // Downloads that run unseen or travel in the clear.
        let fetches = line.contains("curl") || line.contains("wget");
        let piped = ["| sh", "|sh", "| bash", "|bash", "| sudo", "| python"].iter().any(|p| line.contains(p));
        if fetches && piped {
            s.hit(&FETCH, at, "a download is piped straight into an interpreter");
        } else if line.contains("http://") && !LOCAL.iter().any(|l| line.contains(l)) && FETCHERS.iter().any(|f| line.contains(f)) {
            s.hit(&FETCH, at, "a build dependency is fetched over cleartext http");
        }
    }
}

fn config_keys(s: &mut Sink) {
    let cx = s.cx;
    for (line, key, value) in kconfig(cx.text) {
        let (y, n) = (value == "y", value == "n");
        if (y && BOOT_MUST_NOT_BE_Y.contains(&key)) || (n && BOOT_MUST_NOT_BE_N.contains(&key)) {
            s.hit(&BOOT_VERIFY, line, format!("{key}={value} weakens boot or update verification"));
        } else if n && ROLLBACK_MUST_NOT_BE_N.contains(&key) {
            s.hit(&ROLLBACK, line, format!("{key}=n lets an older, vulnerable image be installed"));
        } else if y && DEBUG_MUST_NOT_BE_Y.contains(&key) {
            s.hit(&DEBUG_OPEN, line, format!("{key}=y leaves JTAG open on a secure-boot device"));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{check, check_change, is_advisory, render};
    use crate::model::{ChangeInfo, Pack};
    use crate::profile::Profile;

    fn declared(breaking: bool) -> ChangeInfo {
        ChangeInfo { known: true, breaking, issues: String::new(), why: String::new() }
    }
    fn change_ids(p: &Profile, files: &[(&str, &str)], c: &ChangeInfo) -> Vec<&'static str> {
        check_change(Pack::Integrity, files, p, c).iter().map(|f| f.rule.id).collect()
    }

    #[test]
    fn fwi008_layout_change_not_declared_warns() {
        let p = Profile::parse("[compatibility]\nlayout_paths = [\"src/nvm/\", \"*_layout.h\"]\n").unwrap();
        for path in ["STM32F407VGTx_FLASH.ld", "partitions.csv", "pm_static.yml", "boards/x/flash_map.dtsi", "src/nvm/records.h", "include/cfg_layout.h"] {
            let f = check_change(Pack::Integrity, &[(path, "")], &p, &declared(false));
            assert_eq!(f.iter().map(|f| f.rule.id).collect::<Vec<_>>(), ["EMB-FWI-008"], "{path}");
            assert!(is_advisory(&f[0], &p));
            assert!(crate::engine::render_warning(&f[0]).1.contains("RFC 9124 4.3.2"));
        }
        assert!(change_ids(&p, &[("src/main.c", "int x;\n")], &declared(false)).is_empty());
        assert!(change_ids(&p, &[("third_party/sdk/link.ld", "")], &declared(false)).is_empty());
    }
    #[test]
    fn fwi009_breaking_layout_change_needs_a_version_change() {
        let p = Profile::parse("[compatibility]\nversion_paths = [\"fw.toml\"]\n").unwrap();
        let f = check_change(Pack::Integrity, &[("app.ld", "")], &p, &declared(true));
        assert_eq!(f.iter().map(|f| f.rule.id).collect::<Vec<_>>(), ["EMB-FWI-009"]);
        assert!(!is_advisory(&f[0], &p));
        assert!(render(&f, &p).1.contains("UN Regulation No. 156 7.1.1.7"));
        for v in ["VERSION", "include/app_version.h", "src/fw_version.h", "fw.toml"] {
            assert!(change_ids(&p, &[("app.ld", ""), (v, "1.2.0")], &declared(true)).is_empty(), "{v}");
        }
        // A justified deviation in the layout file waives it.
        let dev = "/* EMB-DEVIATION(EMB-FWI-009): only the debug-only RAM region moved, DR-40 */\n";
        assert!(change_ids(&p, &[("app.ld", dev)], &declared(true)).is_empty());
    }
    #[test]
    fn fwi_intent_absent() {
        assert!(change_ids(&Profile::default(), &[("app.ld", "")], &ChangeInfo::unknown()).is_empty());
        assert!(check(Pack::Integrity, &[("partitions.csv", "")], &Profile::default()).is_empty());
    }

    fn ids_at(path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Integrity, &[(path, src)], &Profile::default()).iter().map(|f| f.rule.id).collect()
    }
    fn rule(id: &str, clause: &str, path: &str, bad: &str, good: &str) {
        let p = Profile::default();
        let f = check(Pack::Integrity, &[(path, bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids_at(path, bad));
        assert!(render(&f, &p).1.contains(clause));
        assert!(!ids_at(path, good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn fwi001_warnings_disabled() {
        rule("EMB-FWI-001", "Power of 10 Rule 10; CWE-1127; NIST SP 800-218 PW.6.2", "Makefile", "CFLAGS += -O2 -w\n", "CFLAGS += -O2 -Wall -Wextra -Werror -Wno-unused-parameter\n");
        for (path, bad) in [
            ("CMakeLists.txt", "target_compile_options(app PRIVATE -Wno-error)\n"),
            ("CMakeLists.txt", "add_compile_options(\"-Wno-implicit-function-declaration\")\n"),
            ("platformio.ini", "build_flags = -Wno-error=return-type -fpermissive\n"),
            ("build.mk", "CFLAGS += -Wno-format-security\n"),
        ] {
            assert!(ids_at(path, bad).contains(&"EMB-FWI-001"), "{bad}");
        }
    }
    #[test]
    fn fwi002_hardening_disabled() {
        rule("EMB-FWI-002", "NIST SP 800-218 PW.6.1, PW.6.2", "Makefile", "CFLAGS += -fno-stack-protector\n", "CFLAGS += -fstack-protector-strong -D_FORTIFY_SOURCE=2\n");
        assert!(ids_at("CMakeLists.txt", "target_link_options(app PRIVATE -Wl,-z,execstack)\n").contains(&"EMB-FWI-002"));
        assert!(ids_at("Makefile", "CFLAGS += -D_FORTIFY_SOURCE=0\n").contains(&"EMB-FWI-002"));
    }
    #[test]
    fn fwi003_boot_verification_disabled() {
        rule(
            "EMB-FWI-003",
            "ETSI EN 303 645 Provision 5.7-1, 5.3-10; NIST SP 800-193 4.2.1.3; UN Regulation No. 156 7.2.1.1; CWE-494",
            "sdkconfig.defaults",
            "CONFIG_SECURE_BOOT_INSECURE=y\n",
            "CONFIG_SECURE_BOOT=y\nCONFIG_SECURE_FLASH_ENC_ENABLED=y\n# CONFIG_SECURE_BOOT_INSECURE is not set\n",
        );
        for bad in ["# CONFIG_SECURE_BOOT is not set\n", "CONFIG_BOOT_SIGNATURE_TYPE_NONE=y\n", "CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT=y\n", "CONFIG_BOOT_VALIDATE_SLOT0=n\n"] {
            assert!(ids_at("prj.conf", bad).contains(&"EMB-FWI-003"), "{bad}");
        }
        // Mandatory.
        assert_eq!(ids_at("prj.conf", "# EMB-DEVIATION(EMB-FWI-003): bring-up board without eFuses burned\nCONFIG_SECURE_BOOT=n\n"), ["EMB-FWI-003"]);
    }
    #[test]
    fn fwi004_rollback_protection_disabled() {
        rule("EMB-FWI-004", "NIST SP 800-193 4.2.1.3", "sdkconfig.defaults", "CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=n\n", "CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=y\n");
        assert!(ids_at("prj.conf", "# CONFIG_MCUBOOT_DOWNGRADE_PREVENTION is not set\n").contains(&"EMB-FWI-004"));
    }
    #[test]
    fn fwi005_development_signing_key() {
        rule(
            "EMB-FWI-005",
            "ETSI EN 303 645 Provision 5.4-3; CWE-798",
            "prj.conf",
            "CONFIG_BOOT_SIGNATURE_KEY_FILE=\"bootloader/mcuboot/root-rsa-2048.pem\"\n",
            "CONFIG_BOOT_SIGNATURE_KEY_FILE=\"/secure/keys/product-root.pem\"\n",
        );
        assert!(ids_at("CMakeLists.txt", "set(mcuboot_key_file ${ZEPHYR_MCUBOOT_MODULE_DIR}/root-ec-p256.pem)\n").contains(&"EMB-FWI-005"));
        assert!(ids_at("src/sign.c", "const char *k = \"root-ed25519.pem\";").contains(&"EMB-FWI-005"));
    }
    #[test]
    fn fwi006_unauthenticated_fetch() {
        rule(
            "EMB-FWI-006",
            "CWE-494, CWE-829; NIST SP 800-218 PW.4.1",
            "scripts/setup.sh",
            "curl -sSf https://sh.vendor.io/install | sh\n",
            "curl -sSfLo tool.tgz https://dl.vendor.io/tool.tgz\necho \"$SHA  tool.tgz\" | sha256sum -c -\n",
        );
        assert!(ids_at("CMakeLists.txt", "FetchContent_Declare(lib URL http://dl.vendor.io/lib.tar.gz)\n").contains(&"EMB-FWI-006"));
        assert!(ids_at(".github/workflows/ci.yml", "      - run: wget -qO- http://get.vendor.io | bash\n").contains(&"EMB-FWI-006"));
        assert!(ids_at("scripts/dev.sh", "curl http://localhost:8080/health\n").is_empty());
    }
    #[test]
    fn fwi007_debug_access_open() {
        rule("EMB-FWI-007", "NIST SP 800-193 4.2.1.3; ETSI EN 303 645 Provision 5.6-3; CWE-489", "sdkconfig.defaults", "CONFIG_SECURE_BOOT_ALLOW_JTAG=y\n", "# CONFIG_SECURE_BOOT_ALLOW_JTAG is not set\n");
        assert!(ids_at("src/ob.c", "void f(void) { ob.RDPLevel = OB_RDP_LEVEL_0; }").contains(&"EMB-FWI-007"));
        assert!(ids_at("src/ob.c", "void f(void) { ob.RDPLevel = OB_RDP_LEVEL_1; }").is_empty());
    }

    #[test]
    fn fwi_does_not_flag_hardened_builds() {
        let cmake = "# never use -w or -fno-stack-protector here\ncmake_minimum_required(VERSION 3.20)\nadd_compile_options(-Wall -Wextra -Werror -Wno-unused-parameter -fstack-protector-strong)\ntarget_link_options(app PRIVATE -Wl,-z,relro,-z,now -Wl,--gc-sections)\nFetchContent_Declare(lib URL https://dl.vendor.io/lib.tar.gz URL_HASH SHA256=abc)\n";
        assert_eq!(ids_at("CMakeLists.txt", cmake), Vec::<&str>::new());
        let conf = "CONFIG_SECURE_BOOT=y\nCONFIG_SECURE_BOOT_V2_ENABLED=y\nCONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=y\nCONFIG_WIFI=n\n# CONFIG_FOO is not set\n";
        assert_eq!(ids_at("sdkconfig.defaults", conf), Vec::<&str>::new());
        assert_eq!(ids_at("Makefile", "all:\n\t$(CC) -Wall -o app main.c\nnew-widget: w.c\n"), Vec::<&str>::new());
    }
    #[test]
    fn fwi_scope() {
        assert!(!ids_at("Makefile", "CFLAGS += -w\n").is_empty());
        for p in ["tests/Makefile", "third_party/lib/Makefile", "docs/build.md", "vendor/sdk/CMakeLists.txt"] {
            assert_eq!(ids_at(p, "CFLAGS += -w -fno-stack-protector\n"), Vec::<&str>::new(), "{p}");
        }
    }
    #[test]
    fn malformed_fwi_input_never_panics() {
        for src in ["=", "-", "-Wno-", "CONFIG_=y", "curl |", "\"\"\"(((", "é -w ü"] {
            for p in ["Makefile", "a.conf", "a.sh", "a.c"] {
                let _ = ids_at(p, src);
            }
        }
    }
}
