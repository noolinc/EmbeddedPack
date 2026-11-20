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

impl Profile {
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
                "safety.iec61508_sil" => p.sil = num(v).filter(|s| *s <= 4).ok_or_else(bad)? as u8,
                _ => return Err(format!("line {}: unknown key {key}", n + 1)),
            }
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
    }
}
