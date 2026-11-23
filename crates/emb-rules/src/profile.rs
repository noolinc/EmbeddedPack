//! The hardware profile the policies are built against.
//!
//! A policy runs in a sandbox and cannot read the repository, so the profile
//! is compiled in (`hardware-profile.toml` at the repository root). Rebuild
//! and reinstall the policies when the hardware changes.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exec {
    /// One `main` loop, no kernel: nothing may block.
    Superloop,
    /// Preemptive tasks: block in the kernel, never busy-wait.
    Rtos,
    /// Run-to-completion handlers dispatched from a queue.
    EventDriven,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Heap {
    None,
    /// Allocate during start-up only, never free.
    InitOnly,
    Allowed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fpu {
    None,
    Single,
    Double,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Profile {
    pub name: String,
    pub ram_bytes: u64,
    pub flash_bytes: u64,
    pub fpu: Fpu,
    pub exec: Exec,
    pub heap: Heap,
    /// 0 = derive from RAM.
    pub max_stack_object_bytes: u64,
    pub max_function_lines: u32,
    /// 0 = QM, 1..=4 = ASIL A..D.
    pub asil: u8,
    /// 0 = none, 1..=3 = IEC 62304 class A..C.
    pub iec62304_class: u8,
    /// IEC 61508 / EN 50128 SIL, 0..=4.
    pub sil: u8,
    /// Files larger than this are not analysed (`[analysis] max_file_bytes`).
    pub max_file_bytes: u64,
    /// The guideline re-categorization plan (`[recategorize]`).
    pub recategorized: Vec<(String, Recat)>,
    /// `[language] c_standard`: the C edition as a year (1990, 1999, 2011,
    /// 2017, 2023); `None` gates nothing.
    pub c_standard: Option<u16>,
    /// `[language] cpp_standard`: the C++ edition as a year (2011..2023).
    pub cpp_standard: Option<u16>,
    /// `[language] headers = "cpp"`: `.h` files are C++.
    pub headers_cpp: bool,
    /// `[compatibility] layout_paths`: files whose layout persists across
    /// updates (NVM records, flash maps), on top of the built-in names.
    pub layout_paths: Vec<String>,
    /// `[compatibility] version_paths`: files that carry the firmware or
    /// schema version, on top of the built-in names.
    pub version_paths: Vec<String>,
    /// `[evidence] target_assertion`: the attestation that proves a change
    /// ran on the target or a high-fidelity simulation. `None` = rule off.
    pub target_assertion: Option<String>,
    /// `[evidence] target_environments`: environments that count.
    pub target_environments: Vec<String>,
    /// `[evidence] target_paths`: hardware-facing paths beyond the built-ins.
    pub target_paths: Vec<String>,
    /// `[images] roots`: one directory per separately flashed image.
    pub image_roots: Vec<String>,
    /// `[budget]` percentages of flash and RAM: warn at, fail above.
    pub flash_warn_percent: u8,
    pub flash_max_percent: u8,
    pub ram_warn_percent: u8,
    pub ram_max_percent: u8,
}

/// Paths are relative to the repository root. `*` matches any run of
/// characters, `/` included; a pattern ending in `/` matches everything under
/// that directory. Iterative, no recursion.
pub fn glob_match(pat: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    if pat.ends_with('/') && !pat.contains('*') {
        return path.starts_with(pat);
    }
    let owned;
    let pat = if pat.ends_with('/') {
        owned = format!("{pat}*");
        owned.as_str()
    } else {
        pat
    };
    let (p, t) = (pat.as_bytes(), path.as_bytes());
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            while pi < p.len() && p[pi] == b'*' {
                pi += 1;
            }
            star = pi;
            mark = ti;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if star != usize::MAX {
            mark += 1;
            ti = mark;
            pi = star;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Matches `path` or any tail of it that starts at a directory boundary, so a
/// repository-relative pattern also fits an absolute path.
pub fn any_glob(pats: &[String], path: &str) -> bool {
    let tails = std::iter::once(path).chain(path.match_indices('/').map(|(i, _)| &path[i + 1..]));
    tails.into_iter().any(|t| pats.iter().any(|p| glob_match(p, t)))
}

/// `["a", "b"]` on one line.
fn list(v: &str) -> Option<Vec<String>> {
    let inner = v.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut out = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let item = item.strip_prefix('"')?.strip_suffix('"')?;
        out.push(item.to_string());
    }
    Some(out)
}

fn c_year(v: &str) -> Option<u16> {
    let v = v.strip_prefix("gnu").or_else(|| v.strip_prefix('c'))?;
    Some(match v {
        "89" | "90" => 1990,
        "99" => 1999,
        "11" => 2011,
        "17" | "18" => 2017,
        "23" | "2x" => 2023,
        _ => return None,
    })
}

fn cpp_year(v: &str) -> Option<u16> {
    let v = v.strip_prefix("gnu++").or_else(|| v.strip_prefix("c++"))?;
    Some(match v {
        "11" | "0x" => 2011,
        "14" | "1y" => 2014,
        "17" | "1z" => 2017,
        "20" | "2a" => 2020,
        "23" | "2b" => 2023,
        _ => return None,
    })
}

/// A C edition as `-std=` spells it, for messages.
pub fn c_name(year: u16) -> &'static str {
    match year {
        1990 => "C90",
        1999 => "C99",
        2011 => "C11",
        2017 => "C17",
        _ => "C23",
    }
}

pub fn cpp_name(year: u16) -> &'static str {
    match year {
        2011 => "C++11",
        2014 => "C++14",
        2017 => "C++17",
        2020 => "C++20",
        _ => "C++23",
    }
}

/// Parse a `-std=` value or `CMAKE_C_STANDARD`-style number for C.
pub fn parse_c_standard(v: &str) -> Option<u16> {
    let v = v.trim().to_ascii_lowercase();
    c_year(&v).or_else(|| c_year(&format!("c{v}")))
}

pub fn parse_cpp_standard(v: &str) -> Option<u16> {
    let v = v.trim().to_ascii_lowercase();
    cpp_year(&v).or_else(|| cpp_year(&format!("c++{v}")))
}

/// A revised category in the guideline re-categorization plan.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recat {
    Mandatory,
    Required,
    Advisory,
    /// Not checked at all.
    Disapplied,
}

/// Whether MISRA Compliance:2020 5.1 permits moving a guideline from `from`
/// to `to`: Required may only become Mandatory; Advisory may become anything;
/// Mandatory may not move.
pub fn recategorization_allowed(from: crate::model::Category, to: Recat) -> bool {
    use crate::model::Category as C;
    matches!(
        (from, to),
        (C::Mandatory, Recat::Mandatory) | (C::Required, Recat::Mandatory | Recat::Required) | (C::Advisory, _)
    )
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            name: "unspecified".into(),
            ram_bytes: 64 * 1024,
            flash_bytes: 256 * 1024,
            fpu: Fpu::None,
            exec: Exec::Superloop,
            heap: Heap::None,
            max_stack_object_bytes: 0,
            max_function_lines: 60,
            asil: 0,
            iec62304_class: 0,
            sil: 0,
            max_file_bytes: 2 * 1024 * 1024,
            recategorized: Vec::new(),
            c_standard: None,
            cpp_standard: None,
            headers_cpp: false,
            layout_paths: Vec::new(),
            version_paths: Vec::new(),
            target_assertion: None,
            target_environments: Vec::new(),
            target_paths: Vec::new(),
            image_roots: Vec::new(),
            flash_warn_percent: 85,
            flash_max_percent: 100,
            ram_warn_percent: 85,
            ram_max_percent: 100,
        }
    }
}

fn num(v: &str) -> Option<u64> {
    let v: String = v.chars().filter(|c| *c != '_').collect();
    let (digits, mul) = match v.to_ascii_lowercase().as_str() {
        s if s.ends_with("kib") || s.ends_with("kb") || s.ends_with('k') => {
            (s.trim_end_matches(|c: char| c.is_ascii_alphabetic()).to_string(), 1024)
        }
        s if s.ends_with("mib") || s.ends_with("mb") || s.ends_with('m') => {
            (s.trim_end_matches(|c: char| c.is_ascii_alphabetic()).to_string(), 1024 * 1024)
        }
        s => (s.to_string(), 1),
    };
    digits.trim().parse::<u64>().ok().map(|n| n.saturating_mul(mul))
}

fn pct(v: &str) -> Option<u8> {
    num(v).filter(|n| (1..=100).contains(n)).map(|n| n as u8)
}

impl Profile {
    /// The category `rule` has under this profile's re-categorization plan;
    /// `None` when it is disapplied.
    pub fn category_of(&self, rule: &crate::model::Rule) -> Option<crate::model::Category> {
        use crate::model::Category as C;
        match self.recategorized.iter().find(|(id, _)| id == rule.id).map(|(_, r)| *r) {
            None => Some(rule.category),
            Some(Recat::Mandatory) => Some(C::Mandatory),
            Some(Recat::Required) => Some(C::Required),
            Some(Recat::Advisory) => Some(C::Advisory),
            Some(Recat::Disapplied) => None,
        }
    }

    /// Parse `key = value` lines under `[section]` headers. An unknown key or
    /// a bad value is an error: a profile that silently falls back to a
    /// default would enforce the wrong hardware.
    pub fn parse(text: &str) -> Result<Profile, String> {
        let mut p = Profile::default();
        let mut section = String::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(s) = line.strip_prefix('[') {
                section = s.trim_end_matches(']').trim().to_string();
                continue;
            }
            let (k, v) = line.split_once('=').ok_or(format!("line {}: expected key = value", n + 1))?;
            let v = v.trim().trim_matches('"');
            if section == "recategorize" {
                let id = k.trim().trim_matches('"');
                let rule = crate::engine::rule_by_id(id).ok_or(format!("line {}: [recategorize] names no rule '{id}'", n + 1))?;
                let to = match v.to_ascii_lowercase().as_str() {
                    "mandatory" => Recat::Mandatory,
                    "required" => Recat::Required,
                    "advisory" => Recat::Advisory,
                    "disapplied" => Recat::Disapplied,
                    _ => return Err(format!("line {}: bad category '{v}' for {id} (mandatory | required | advisory | disapplied)", n + 1)),
                };
                if !recategorization_allowed(rule.category, to) {
                    return Err(format!(
                        "line {}: {id} is {:?} and cannot become {to:?} (MISRA Compliance:2020 5.1: a Mandatory guideline is never re-categorized; a Required one only to Mandatory)",
                        n + 1,
                        rule.category
                    ));
                }
                p.recategorized.retain(|(r, _)| r != id);
                p.recategorized.push((id.to_string(), to));
                continue;
            }
            let key = format!("{section}.{}", k.trim());
            let bad = || format!("line {}: bad value '{v}' for {key}", n + 1);
            let lv = v.to_ascii_lowercase();
            match key.as_str() {
                "hardware.name" => p.name = v.to_string(),
                "hardware.arch" => {}
                "hardware.ram_bytes" => p.ram_bytes = num(v).ok_or_else(bad)?,
                "hardware.flash_bytes" => p.flash_bytes = num(v).ok_or_else(bad)?,
                "hardware.fpu" => {
                    p.fpu = match lv.as_str() {
                        "none" => Fpu::None,
                        "single" => Fpu::Single,
                        "double" => Fpu::Double,
                        _ => return Err(bad()),
                    }
                }
                "architecture.execution_model" => {
                    p.exec = match lv.as_str() {
                        "superloop" | "main-loop" => Exec::Superloop,
                        "rtos" => Exec::Rtos,
                        "event-driven" => Exec::EventDriven,
                        _ => return Err(bad()),
                    }
                }
                "architecture.heap" => {
                    p.heap = match lv.as_str() {
                        "none" => Heap::None,
                        "init-only" => Heap::InitOnly,
                        "allowed" => Heap::Allowed,
                        _ => return Err(bad()),
                    }
                }
                "architecture.max_stack_object_bytes" => p.max_stack_object_bytes = num(v).ok_or_else(bad)?,
                "architecture.max_function_lines" => p.max_function_lines = num(v).ok_or_else(bad)? as u32,
                "safety.iso26262_asil" => {
                    p.asil = match lv.as_str() {
                        "qm" | "none" => 0,
                        "a" => 1,
                        "b" => 2,
                        "c" => 3,
                        "d" => 4,
                        _ => return Err(bad()),
                    }
                }
                "safety.iec62304_class" => {
                    p.iec62304_class = match lv.as_str() {
                        "none" => 0,
                        "a" => 1,
                        "b" => 2,
                        "c" => 3,
                        _ => return Err(bad()),
                    }
                }
                "analysis.max_file_bytes" => p.max_file_bytes = num(v).filter(|n| *n > 0).ok_or_else(bad)?,
                "safety.iec61508_sil" => p.sil = num(v).filter(|s| *s <= 4).ok_or_else(bad)? as u8,
                "language.c_standard" => p.c_standard = Some(parse_c_standard(v).ok_or_else(bad)?),
                "language.cpp_standard" => p.cpp_standard = Some(parse_cpp_standard(v).ok_or_else(bad)?),
                "language.headers" => {
                    p.headers_cpp = match lv.as_str() {
                        "c" => false,
                        "cpp" | "c++" => true,
                        _ => return Err(bad()),
                    }
                }
                "compatibility.layout_paths" => p.layout_paths = list(v).ok_or_else(bad)?,
                "compatibility.version_paths" => p.version_paths = list(v).ok_or_else(bad)?,
                "evidence.target_assertion" => p.target_assertion = Some(v.to_string()).filter(|s| !s.is_empty()),
                "evidence.target_environments" => p.target_environments = list(v).ok_or_else(bad)?,
                "evidence.target_paths" => p.target_paths = list(v).ok_or_else(bad)?,
                "images.roots" => p.image_roots = list(v).ok_or_else(bad)?,
                "budget.flash_warn_percent" => p.flash_warn_percent = pct(v).ok_or_else(bad)?,
                "budget.flash_max_percent" => p.flash_max_percent = pct(v).ok_or_else(bad)?,
                "budget.ram_warn_percent" => p.ram_warn_percent = pct(v).ok_or_else(bad)?,
                "budget.ram_max_percent" => p.ram_max_percent = pct(v).ok_or_else(bad)?,
                _ => return Err(format!("line {}: unknown key {key}", n + 1)),
            }
        }
        if p.flash_warn_percent > p.flash_max_percent || p.ram_warn_percent > p.ram_max_percent {
            return Err("[budget]: a warn percentage is above its max percentage".into());
        }
        if p.target_assertion.is_some() && p.target_environments.is_empty() {
            return Err("[evidence]: target_assertion needs target_environments (for example [\"hil\"])".into());
        }
        Ok(p)
    }

    /// The largest object a function may put on its stack.
    pub fn stack_object_limit(&self) -> u64 {
        if self.max_stack_object_bytes > 0 {
            self.max_stack_object_bytes
        } else {
            (self.ram_bytes / 64).clamp(64, 4096)
        }
    }

    /// Flash too small for the formatted-I/O and stream libraries.
    pub fn small_flash(&self) -> bool {
        self.flash_bytes < 128 * 1024
    }

    /// ASIL C/D, IEC 62304 class C, SIL 3/4: required rules stop being deviatable.
    pub fn high_integrity(&self) -> bool {
        self.asil >= 3 || self.iec62304_class >= 3 || self.sil >= 3
    }

    /// Any safety classification that demands requirement traceability.
    pub fn needs_traceability(&self) -> bool {
        self.asil >= 1 || self.iec62304_class >= 2 || self.sil >= 1
    }

    pub fn summary(&self) -> String {
        format!(
            "{} (RAM {} B, flash {} B, fpu {:?}, {:?}, heap {:?}, ASIL {}, 62304 class {}, SIL {})",
            self.name,
            self.ram_bytes,
            self.flash_bytes,
            self.fpu,
            self.exec,
            self.heap,
            ["QM", "A", "B", "C", "D"][self.asil.min(4) as usize],
            ["none", "A", "B", "C"][self.iec62304_class.min(3) as usize],
            self.sil
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_units_and_levels() {
        let p = Profile::parse(
            "[hardware]\nname = \"stm32f030\"\nram_bytes = 4k # tiny\nflash_bytes = 16_384\nfpu = \"none\"\n\
             [architecture]\nexecution_model = \"superloop\"\nheap = \"none\"\n[safety]\niso26262_asil = \"D\"\niec62304_class = \"C\"\niec61508_sil = 3\n",
        )
        .unwrap();
        assert_eq!((p.ram_bytes, p.flash_bytes), (4096, 16384));
        assert_eq!(p.stack_object_limit(), 64);
        assert!(p.small_flash() && p.high_integrity() && p.needs_traceability());
    }

    #[test]
    fn rejects_unknown_keys_and_bad_values() {
        assert!(Profile::parse("[hardware]\nrom = 1").is_err());
        assert!(Profile::parse("[architecture]\nheap = \"some\"").is_err());
        assert!(Profile::parse("[safety]\niec61508_sil = 9").is_err());
        assert!(Profile::parse("[analysis]\nmax_file_bytes = 0").is_err());
    }

    #[test]
    fn budget_percentages_parse_and_are_checked() {
        let d = Profile::default();
        assert_eq!((d.flash_warn_percent, d.flash_max_percent, d.ram_warn_percent, d.ram_max_percent), (85, 100, 85, 100));
        let p = Profile::parse("[budget]\nflash_warn_percent = 80\nflash_max_percent = 95\nram_warn_percent = 70\nram_max_percent = 90\n").unwrap();
        assert_eq!((p.flash_warn_percent, p.flash_max_percent, p.ram_warn_percent, p.ram_max_percent), (80, 95, 70, 90));
        for bad in ["[budget]\nflash_warn_percent = 0\n", "[budget]\nram_max_percent = 101\n", "[budget]\nflash_warn_percent = 96\nflash_max_percent = 95\n"] {
            assert!(Profile::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn language_lists_and_globs() {
        let p = Profile::parse("[language]\nc_standard = \"gnu99\"\ncpp_standard = \"c++17\"\nheaders = \"cpp\"\n[images]\nroots = [\"bootloader/\", \"app/\"]\n").unwrap();
        assert_eq!((p.c_standard, p.cpp_standard, p.headers_cpp), (Some(1999), Some(2017), true));
        assert_eq!(p.image_roots, ["bootloader/", "app/"]);
        assert!(Profile::parse("[language]\nc_standard = \"c12\"\n").is_err());
        assert!(Profile::parse("[images]\nroots = bootloader\n").is_err());
        let g = |pats: &[&str], path: &str| any_glob(&pats.iter().map(|s| s.to_string()).collect::<Vec<_>>(), path);
        assert!(g(&["src/nvm/"], "src/nvm/rec.h") && g(&["src/nvm/"], "/abs/repo/src/nvm/rec.h") && !g(&["src/nvm/"], "src/nvmx/a.h"));
        assert!(g(&["*_layout.h"], "include/cfg_layout.h") && g(&["drivers/*/regs.h"], "drivers/spi/regs.h") && !g(&["*.ld"], "a.ldx"));
    }

    #[test]
    fn max_file_bytes_defaults_to_2_mib_and_parses_units() {
        assert_eq!(Profile::default().max_file_bytes, 2 * 1024 * 1024);
        let p = Profile::parse("[analysis]\nmax_file_bytes = 8m").unwrap();
        assert_eq!(p.max_file_bytes, 8 * 1024 * 1024);
    }
}
