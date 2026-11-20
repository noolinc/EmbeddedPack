//! `emb_security_baseline`: device security defects a reviewer can see in
//! the text of a change.
//!
//! Clauses were checked against ETSI EN 303 645 V3.1.3, the CWE-699 data set,
//! and the CERT ids in MISRA C:2012 Addendum 3 and Amendment 2.

use crate::engine::Sink;
use crate::lex::Kind;
use crate::model::{Category, FileCtx, Lang, Pack, Rule};

macro_rules! rules {
    ($( $name:ident = $id:literal, $cat:ident, $title:literal, $refs:literal; )*) => {
        $( pub static $name: Rule = Rule {
            id: $id, pack: Pack::Security, category: Category::$cat, strict: true, title: $title, refs: $refs,
        }; )*
        pub static RULES: &[&Rule] = &[ $( &$name ),* ];
    };
}

rules! {
    CREDENTIAL = "EMB-SEC-001", Mandatory, "credentials and keys shall not be hard-coded",
        "ETSI EN 303 645 Provision 5.1-1, 5.4-3; CWE-798";
    PRIVATE_KEY = "EMB-SEC-002", Mandatory, "private key material shall not be stored in the repository",
        "ETSI EN 303 645 Provision 5.4-1, 5.4-3; CWE-798";
    WEAK_CRYPTO = "EMB-SEC-003", Required, "broken or weak cryptographic algorithms shall not be used",
        "ETSI EN 303 645 Provision 5.5-1; CWE-328";
    WEAK_PRNG = "EMB-SEC-004", Required, "rand() shall not be used",
        "CWE-338; SEI CERT C MSC30-C, MSC32-C; AUTOSAR C++14 A26-5-1";
    GETS = "EMB-SEC-005", Mandatory, "gets() shall not be used",
        "CWE-242";
    UNBOUNDED = "EMB-SEC-006", Required, "unbounded string functions shall not be used",
        "CWE-120, CWE-676; SEI CERT C STR31-C";
    TLS_VERIFY = "EMB-SEC-007", Mandatory, "certificate verification shall not be disabled",
        "ETSI EN 303 645 Provision 5.5-1; CWE-295";
    CLEARTEXT = "EMB-SEC-008", Required, "endpoints shall not use cleartext protocols",
        "ETSI EN 303 645 Provision 5.5-1, 5.5-7; CWE-319";
    FORMAT = "EMB-SEC-009", Required, "a format string shall be a literal",
        "CWE-134; SEI CERT C FIO30-C";
    SECRET_COMPARE = "EMB-SEC-010", Required, "secrets shall be compared in constant time",
        "CWE-208";
    COMMAND = "EMB-SEC-011", Required, "commands shall not be run through a shell or exec",
        "CWE-78; SEI CERT C ENV33-C; MISRA C:2012 Rule 21.21 (Amendment 2)";
}

const SECRET_WORDS: [&str; 22] = [
    "password", "passwd", "passphrase", "secret", "api_key", "apikey", "private_key", "privkey", "priv_key", "auth_token",
    "access_token", "psk", "aes_key", "hmac_key", "enc_key", "encryption_key", "signing_key", "master_key", "root_key",
    "device_key", "wifi_pass", "credential",
];
/// Words that mark a name as being about a secret, not holding one. Matched
/// as whole `_`-separated words: `admin_passwd` must not match `min`.
const NOT_A_VALUE: [&str; 33] = [
    "len", "length", "size", "file", "path", "prompt", "label", "name", "id", "slot", "type", "max", "min", "hint", "header",
    "prefix", "topic", "url", "public", "pub", "field", "param", "attr", "format", "fmt", "policy", "rule", "error", "msg",
    "required", "enabled", "timeout", "count",
];
/// The same idea for camelCase names, where there is no separator.
const NOT_A_VALUE_INFIX: [&str; 6] = ["length", "prompt", "public", "label", "filename", "filepath"];

pub(crate) fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let hit = SECRET_WORDS.iter().any(|w| n.contains(w)) || n == "pwd" || n.ends_with("_pwd") || n == "pass" || n.ends_with("_pass");
    hit && !n.split('_').any(|seg| NOT_A_VALUE.contains(&seg)) && !NOT_A_VALUE_INFIX.iter().any(|w| n.contains(w))
}

const SECRET_OPERANDS: [&str; 11] = ["password", "passwd", "secret", "hmac", "digest", "signature", "auth_tag", "token", "psk", "pin_code", "cmac"];
const UNBOUNDED_FNS: [&str; 7] = ["strcpy", "strcat", "sprintf", "vsprintf", "stpcpy", "wcscpy", "wcscat"];
const COMMAND_FNS: [&str; 8] = ["system", "popen", "execl", "execlp", "execle", "execv", "execvp", "execvpe"];
const CLEARTEXT_SCHEMES: [&str; 6] = ["http://", "ftp://", "telnet://", "mqtt://", "ws://", "tftp://"];
const NOT_AN_ENDPOINT: [&str; 10] = [
    "localhost", "127.", "[::1]", "0.0.0.0", "www.w3.org", "schemas.", "json-schema.org", "apache.org/licenses", "xmlsoap.org", "example.",
];
const INSECURE_TLS_KEYS: [&str; 2] = ["CONFIG_ESP_TLS_INSECURE", "CONFIG_ESP_TLS_SKIP_SERVER_CERT_VERIFY"];

pub fn run(s: &mut Sink) {
    let cx = s.cx;
    if !cx.is_production() {
        return;
    }
    private_keys(s);
    match cx.lang {
        Lang::C | Lang::Cpp => {
            credentials_in_code(s);
            calls(s);
            identifiers_and_strings(s);
        }
        Lang::Build | Lang::Config | Lang::Script => config_lines(s),
        Lang::Other => {}
    }
}

fn private_keys(s: &mut Sink) {
    let cx = s.cx;
    // The two halves of a PEM header, kept apart so this file is not one.
    let begin = "-----BEGIN ";
    let tail = "PRIVATE KEY-----";
    for (n, line) in cx.text.lines().enumerate() {
        if line.contains(begin) && line.contains(tail) {
            s.hit(&PRIVATE_KEY, n as u32 + 1, "PEM private key");
            return;
        }
    }
}

fn credentials_in_code(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.kind != Kind::Ident || !is_secret_name(tok.text) {
            continue;
        }
        // `#define WIFI_PASSWORD "..."`
        if tok.pp && i >= 2 && t[i - 1].is("define") && t[i - 2].kind == Kind::Hash {
            if t.get(i + 1).and_then(|v| v.str_body()).is_some_and(|b| !b.is_empty()) {
                s.hit(&CREDENTIAL, tok.line, format!("{} is defined as a literal", tok.text));
            }
            continue;
        }
        // `name = "..."`, `name[] = "..."`, `.name = "..."`, `name[16] = { 0x.. }`
        let mut k = i + 1;
        if t.get(k).is_some_and(|b| b.is("[")) {
            k = cx.pair(k).map_or(k, |c| c + 1);
        }
        if !t.get(k).is_some_and(|e| e.is("=")) {
            continue;
        }
        match t.get(k + 1) {
            Some(v) if v.str_body().is_some_and(|b| !b.is_empty()) => {
                s.hit(&CREDENTIAL, tok.line, format!("{} is assigned a literal", tok.text));
            }
            Some(v) if v.is("{") => {
                let close = cx.pair(k + 1).unwrap_or(k + 1);
                let bytes = (k + 2..close).filter(|&j| t[j].kind == Kind::Number).count();
                if bytes >= 8 {
                    s.hit(&CREDENTIAL, tok.line, format!("{} is initialised with {bytes} literal bytes", tok.text));
                }
            }
            _ => {}
        }
    }
}

/// Top-level arguments of the call whose `(` is at `open`, as token ranges.
fn args(cx: &FileCtx, open: usize) -> Vec<(usize, usize)> {
    let t = cx.toks();
    let Some(close) = cx.pair(open) else { return Vec::new() };
    let mut out = Vec::new();
    let (mut start, mut k) = (open + 1, open + 1);
    while k < close {
        if t[k].is("(") || t[k].is("[") || t[k].is("{") {
            k = cx.pair(k).map_or(k + 1, |p| p + 1);
            continue;
        }
        if t[k].is(",") {
            out.push((start, k));
            start = k + 1;
        }
        k += 1;
    }
    if start < close {
        out.push((start, close));
    }
    out
}

fn calls(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    for (i, tok) in t.iter().enumerate() {
        if tok.pp || tok.kind != Kind::Ident || !cx.is_call(i) {
            continue;
        }
        let name = tok.text;
        let Some(open) = cx.next(i) else { continue };
        let a = args(cx, open);
        match name {
            "gets" => s.hit(&GETS, tok.line, "gets() cannot bound its input"),
            "rand" | "srand" | "random" | "rand_r" => s.hit(&WEAK_PRNG, tok.line, format!("{name}() is not a cryptographic generator")),
            n if UNBOUNDED_FNS.contains(&n) => s.hit(&UNBOUNDED, tok.line, format!("{n}() does not bound the destination")),
            n if COMMAND_FNS.contains(&n) => s.hit(&COMMAND, tok.line, format!("{n}() runs an external command")),
            "scanf" | "sscanf" | "fscanf" => {
                let close = cx.pair(open).unwrap_or(open);
                if (open..close).any(|k| t[k].str_body().is_some_and(|b| b.contains("%s"))) {
                    s.hit(&UNBOUNDED, tok.line, format!("{name}() with an unbounded %s"));
                }
            }
            "memcmp" | "strcmp" | "strncmp" | "bcmp" => {
                let close = cx.pair(open).unwrap_or(open);
                let secret = (open..close).filter(|&k| t[k].kind == Kind::Ident).find(|&k| {
                    let l = t[k].text.to_ascii_lowercase();
                    SECRET_OPERANDS.iter().any(|w| l.contains(w))
                });
                if let Some(k) = secret {
                    s.hit(&SECRET_COMPARE, tok.line, format!("{name}() on '{}' leaks timing; use a constant-time compare", t[k].text));
                }
            }
            _ => {}
        }
        // A format argument that is the last argument and a plain variable.
        let fmt_at = match name {
            "printf" | "vprintf" => Some(0),
            "fprintf" | "sprintf" | "dprintf" | "syslog" => Some(1),
            "snprintf" => Some(2),
            _ => None,
        };
        if let Some(idx) = fmt_at {
            if a.len() == idx + 1 {
                let (b, e) = a[idx];
                if e == b + 1 && t[b].kind == Kind::Ident && t[b].text.chars().any(|c| c.is_ascii_lowercase()) {
                    s.hit(&FORMAT, tok.line, format!("{name}() uses '{}' as its format string", t[b].text));
                }
            }
        }
    }
}

fn weak_algorithm(ident: &str) -> Option<&'static str> {
    let l = ident.to_ascii_lowercase();
    for (needle, name) in [("md5", "MD5"), ("md4", "MD4"), ("arc4", "RC4"), ("rc4", "RC4"), ("sha1", "SHA-1")] {
        if l.contains(needle) {
            return Some(name);
        }
    }
    let multi = l.contains('_') || ident.chars().all(|c| !c.is_ascii_lowercase());
    for seg in l.split('_') {
        match seg {
            "des" | "3des" | "des3" | "tdes" if multi => return Some("DES"),
            "ecb" if multi => return Some("ECB mode"),
            _ => {}
        }
    }
    None
}

fn identifiers_and_strings(s: &mut Sink) {
    let cx = s.cx;
    let t = cx.toks();
    let mut weak = 0;
    for (i, tok) in t.iter().enumerate() {
        match tok.kind {
            Kind::Ident => {
                if weak < 5 {
                    if let Some(alg) = weak_algorithm(tok.text) {
                        s.hit(&WEAK_CRYPTO, tok.line, format!("{alg} ('{}')", tok.text));
                        weak += 1;
                    }
                }
                match tok.text {
                    // A method, so not a free call: `client.setInsecure()`.
                    "setInsecure" if t.get(i + 1).is_some_and(|p| p.is("(")) => {
                        s.hit(&TLS_VERIFY, tok.line, "setInsecure() disables certificate verification")
                    }
                    "MBEDTLS_SSL_VERIFY_NONE" | "SSL_VERIFY_NONE" | "WOLFSSL_VERIFY_NONE" => {
                        s.hit(&TLS_VERIFY, tok.line, format!("{} disables certificate verification", tok.text))
                    }
                    "CURLOPT_SSL_VERIFYPEER" | "CURLOPT_SSL_VERIFYHOST" => {
                        let off = t.get(i + 1).is_some_and(|c| c.is(",")) && t.get(i + 2).is_some_and(|v| matches!(v.text, "0" | "0L" | "false"));
                        if off {
                            s.hit(&TLS_VERIFY, tok.line, format!("{} set to 0", tok.text));
                        }
                    }
                    "skip_cert_common_name_check" => {
                        if t.get(i + 1).is_some_and(|e| e.is("=")) && t.get(i + 2).is_some_and(|v| v.is("true") || v.is("1")) {
                            s.hit(&TLS_VERIFY, tok.line, "skip_cert_common_name_check enabled");
                        }
                    }
                    _ => {}
                }
            }
            Kind::Str if !(tok.pp && i >= 2 && t[i - 1].is("include")) => {
                if let Some(scheme) = tok.str_body().and_then(cleartext_scheme) {
                    s.hit(&CLEARTEXT, tok.line, format!("cleartext {scheme} endpoint"));
                }
            }
            _ => {}
        }
    }
}

fn cleartext_scheme(body: &str) -> Option<&'static str> {
    let scheme = CLEARTEXT_SCHEMES.iter().find(|p| body.starts_with(**p))?;
    let host = &body[scheme.len()..];
    (!host.is_empty() && !NOT_AN_ENDPOINT.iter().any(|h| host.starts_with(h) || body.contains(h))).then_some(*scheme)
}

/// `KEY = value` / `KEY: value` lines in build, config and script files.
fn config_lines(s: &mut Sink) {
    let cx = s.cx;
    for (n, raw) in cx.text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") || line.starts_with(';') {
            continue;
        }
        let Some(at) = line.find(['=', ':']) else { continue };
        let key = line[..at].trim().trim_start_matches("export ").trim_matches(['"', '\'', '-', ' ']);
        let key = key.rsplit([' ', '\t', '.', '(']).next().unwrap_or(key).trim_start_matches("-D");
        let value = line[at + 1..].trim().trim_end_matches([',', ';', ')']).trim();
        let quoted = value.len() >= 2 && (value.starts_with('"') || value.starts_with('\''));
        let bare = value.trim_matches(['"', '\'']);
        if INSECURE_TLS_KEYS.contains(&key) && bare == "y" {
            s.hit(&TLS_VERIFY, n as u32 + 1, format!("{key}=y disables certificate verification"));
            continue;
        }
        if !is_secret_name(key) || bare.is_empty() {
            continue;
        }
        let lower = bare.to_ascii_lowercase();
        let reference = bare.starts_with(['$', '<', '%', '{', '@'])
            || ["environ", "getenv", "secrets.", "vault", "keyring"].iter().any(|w| lower.contains(w))
            || matches!(lower.as_str(), "y" | "n" | "true" | "false" | "null" | "none" | "yes" | "no")
            || (!quoted && (bare.contains('(') || bare.len() < 4));
        if !reference {
            s.hit(&CREDENTIAL, n as u32 + 1, format!("{key} is set to a literal value"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RULES;
    use crate::engine::{check, render};
    use crate::model::Pack;
    use crate::profile::Profile;

    fn ids_at(path: &str, src: &str) -> Vec<&'static str> {
        check(Pack::Security, &[(path, src)], &Profile::default()).iter().map(|f| f.rule.id).collect()
    }
    fn ids(src: &str) -> Vec<&'static str> {
        ids_at("src/net.c", src)
    }
    fn rule(id: &str, clause: &str, bad: &str, good: &str) {
        let p = Profile::default();
        let f = check(Pack::Security, &[("src/net.c", bad)], &p);
        assert!(f.iter().any(|f| f.rule.id == id), "{id} did not fire on: {bad}\n got {:?}", ids(bad));
        assert!(render(&f, &p).1.contains(clause));
        assert!(!ids(good).contains(&id), "{id} fired on: {good}");
    }

    #[test]
    fn sec001_hardcoded_credential() {
        rule(
            "EMB-SEC-001",
            "ETSI EN 303 645 Provision 5.1-1, 5.4-3; CWE-798",
            "static const char *wifi_password = \"hunter2\";",
            "static const char *wifi_password = \"\"; const char *PASSWORD_PROMPT = \"Password: \"; size_t key_len = 16; const char *password = read_nvs();",
        );
        for bad in [
            "#define API_KEY \"abc123\"\n",
            "cfg_t c = { .psk = \"12345678\" };",
            "static const uint8_t aes_key[16] = { 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08 };",
            "char admin_passwd[] = \"admin\";",
        ] {
            assert!(ids(bad).contains(&"EMB-SEC-001"), "{bad}");
        }
        assert!(ids_at("prj.conf", "CONFIG_WIFI_PASSWORD=\"hunter2\"\n").contains(&"EMB-SEC-001"));
        assert!(ids_at("platformio.ini", "build_flags = -DWIFI_PASS=\\\"hunter2\\\"\n").is_empty());
        assert!(ids_at("config/device.yaml", "mqtt:\n  password: s3cretvalue\n").contains(&"EMB-SEC-001"));
        assert!(ids_at("config/device.yaml", "mqtt:\n  password: ${MQTT_PASSWORD}\n  password_file: /run/secret\n").is_empty());
        assert!(ids_at("scripts/flash.sh", "export API_KEY=$(vault read key)\nPASSWORD=$1\n").is_empty());
        // Mandatory: a deviation does not help.
        let dev = "/* EMB-DEVIATION(EMB-SEC-001): factory default, changed on first boot */\nchar admin_passwd[] = \"admin\";";
        assert_eq!(ids(dev), ["EMB-SEC-001"]);
    }
    #[test]
    fn sec002_private_key_material() {
        let pem = format!("{}RSA {}\nMIIB...\n", "-----BEGIN ", "PRIVATE KEY-----");
        let p = Profile::default();
        for path in ["keys/device.pem", "src/key.c", "docs/setup.md"] {
            let f = check(Pack::Security, &[(path, &pem)], &p);
            assert!(f.iter().any(|f| f.rule.id == "EMB-SEC-002"), "{path}");
            assert!(render(&f, &p).1.contains("Provision 5.4-1, 5.4-3"));
        }
        let public = format!("{}PUBLIC KEY-----\n", "-----BEGIN ");
        assert!(ids_at("keys/device.pub.pem", &public).is_empty());
    }
    #[test]
    fn sec003_weak_crypto() {
        rule("EMB-SEC-003", "ETSI EN 303 645 Provision 5.5-1; CWE-328", "void f(void) { mbedtls_md5_init(&c); }", "void f(void) { mbedtls_sha256_init(&c); int des = 0; describe(x); }");
        for bad in ["x = EVP_des_ede3_cbc();", "y = MBEDTLS_MODE_ECB;", "RC4_set_key(&k, 8, d);", "SHA1_Update(&c, d, n);"] {
            assert!(ids(bad).contains(&"EMB-SEC-003"), "{bad}");
        }
    }
    #[test]
    fn sec004_weak_prng() {
        rule("EMB-SEC-004", "CWE-338; SEI CERT C MSC30-C, MSC32-C", "int nonce(void) { return rand(); }", "int nonce(void) { return esp_random(); }");
    }
    #[test]
    fn sec005_gets() {
        rule("EMB-SEC-005", "CWE-242", "void f(char *b) { gets(b); }", "void f(char *b) { fgets(b, 8, stdin); x.gets(b); }");
    }
    #[test]
    fn sec006_unbounded_string() {
        rule("EMB-SEC-006", "CWE-120, CWE-676; SEI CERT C STR31-C", "void f(char *d, const char *s) { strcpy(d, s); }", "void f(char *d, const char *s) { strncpy(d, s, 8); snprintf(d, 8, \"%s\", s); sscanf(s, \"%7s\", d); }");
        assert!(ids("void f(char *d) { sprintf(d, \"%d\", 1); }").contains(&"EMB-SEC-006"));
        assert!(ids("void f(char *d) { scanf(\"%s\", d); }").contains(&"EMB-SEC-006"));
    }
    #[test]
    fn sec007_tls_verification_disabled() {
        rule(
            "EMB-SEC-007",
            "ETSI EN 303 645 Provision 5.5-1; CWE-295",
            "void f(void) { mbedtls_ssl_conf_authmode(&conf, MBEDTLS_SSL_VERIFY_NONE); }",
            "void f(void) { mbedtls_ssl_conf_authmode(&conf, MBEDTLS_SSL_VERIFY_REQUIRED); curl_easy_setopt(c, CURLOPT_SSL_VERIFYPEER, 1L); }",
        );
        assert!(ids("void f(void) { curl_easy_setopt(c, CURLOPT_SSL_VERIFYPEER, 0L); }").contains(&"EMB-SEC-007"));
        assert!(ids_at("src/a.cpp", "void f() { client.setInsecure(); }").contains(&"EMB-SEC-007"));
        assert!(ids_at("sdkconfig.defaults", "CONFIG_ESP_TLS_INSECURE=y\n").contains(&"EMB-SEC-007"));
        assert!(ids_at("sdkconfig.defaults", "# CONFIG_ESP_TLS_INSECURE is not set\nCONFIG_ESP_TLS_INSECURE=n\n").is_empty());
    }
    #[test]
    fn sec008_cleartext_endpoint() {
        rule(
            "EMB-SEC-008",
            "ETSI EN 303 645 Provision 5.5-1, 5.5-7; CWE-319",
            "const char *ota = \"http://updates.vendor.io/fw.bin\";",
            "const char *ota = \"https://updates.vendor.io/fw.bin\"; const char *l = \"http://localhost:8080\"; const char *ns = \"http://www.w3.org/2001/XMLSchema\"; const char *b = \"mqtts://broker\";",
        );
        assert!(ids("const char *b = \"mqtt://broker.vendor.io\";").contains(&"EMB-SEC-008"));
    }
    #[test]
    fn sec009_format_string() {
        rule("EMB-SEC-009", "CWE-134; SEI CERT C FIO30-C", "void f(const char *msg) { printf(msg); }", "void f(const char *msg) { printf(\"%s\", msg); printf(FMT); log_it(msg); }");
        assert!(ids("void f(char *b, const char *m) { snprintf(b, 8, m); }").contains(&"EMB-SEC-009"));
    }
    #[test]
    fn sec010_secret_compare() {
        rule("EMB-SEC-010", "CWE-208", "int ok(const uint8_t *a) { return memcmp(a, expected_hmac, 32) == 0; }", "int ok(const uint8_t *a) { return memcmp(a, header, 4) == 0 && ct_compare(a, expected_hmac, 32); }");
        assert!(ids("int ok(const char *p) { return strcmp(p, stored_password) == 0; }").contains(&"EMB-SEC-010"));
    }
    #[test]
    fn sec011_command_execution() {
        rule("EMB-SEC-011", "CWE-78; SEI CERT C ENV33-C; MISRA C:2012 Rule 21.21 (Amendment 2)", "void f(const char *c) { system(c); }", "void f(void) { file_system(1); os.system = 0; }");
        assert!(ids("void f(void) { FILE *p = popen(cmd, \"r\"); }").contains(&"EMB-SEC-011"));
    }

    #[test]
    fn sec_does_not_flag_safe_code() {
        let src = r#"
#include "http://not-an-endpoint.h"
#define PASSWORD_MAX_LEN 32
#define KEY_FILE_PATH "/etc/keys/device.pem"
static const char *const public_key_pem = "-----BEGIN PUBLIC KEY-----";
static char password[PASSWORD_MAX_LEN];
/* default password is "admin", see http://old.vendor.io and strcpy(a, b) */
int connect_secure(const char *host)
{
    char url[64];
    snprintf(url, sizeof url, "https://%s/api", host);
    strncpy(password, read_nvs("pw"), sizeof password - 1U);
    mbedtls_ssl_conf_authmode(&conf, MBEDTLS_SSL_VERIFY_REQUIRED);
    printf("connecting to %s\n", url);
    return mbedtls_sha256(url, 64, out, 0);
}
"#;
        assert_eq!(ids(src), Vec::<&str>::new());
    }
    #[test]
    fn sec_scope() {
        let bad = "char admin_passwd[] = \"admin\"; void f(char *b) { gets(b); system(b); }";
        assert_eq!(ids(bad).len(), 3);
        for p in ["tests/auth_test.c", "third_party/x.c", "docs/x.md"] {
            assert_eq!(ids_at(p, bad), Vec::<&str>::new(), "{p}");
        }
    }
    #[test]
    fn sec_cites_only_verified_clauses() {
        let unverified = ["5.6-4", "5.13-1", "MSC41", "MSC24", "ERR34", "CWE-259", "CWE-321", "CWE-327", "CWE-693", "CWE-1394", "62443", "21434"];
        for r in RULES {
            for u in unverified {
                assert!(!r.refs.contains(u), "{} cites unverified {u}", r.id);
            }
        }
    }
    #[test]
    fn malformed_sec_input_never_panics() {
        for src in ["password = ", "char k[", "printf(", "memcmp(a, hmac", "#define API_KEY", "x = \"http://", "key[] = {", "é = \"ü\"; printf(é);"] {
            let _ = ids(src);
            let _ = ids_at("a.conf", src);
            let _ = ids_at("a.sh", src);
        }
        let _ = ids_at("a.yaml", "::::\n=\n:\"\npassword:\n'password': '\n");
    }
}
