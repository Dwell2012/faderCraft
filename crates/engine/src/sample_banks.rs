//! Named-bank catalogue. Small metadata files are handled on the control side.
use djcraft_protocol::SampleBank;
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct Banks {
    pub root: Option<PathBuf>,
    pub entries: Vec<SampleBank>,
    pub selected: String,
}
impl Default for Banks {
    fn default() -> Self {
        Self {
            root: None,
            entries: vec![SampleBank {
                id: "default".into(),
                name: "My Samples".into(),
            }],
            selected: "default".into(),
        }
    }
}
impl Banks {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        let mut banks = Self {
            root: Some(root.clone()),
            ..Self::default()
        };
        let text = match std::fs::read_to_string(root.join("banks-v1.txt")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(banks),
            Err(e) => return Err(e.to_string()),
        };
        let mut lines = text.lines();
        if lines.next() != Some("1") {
            return Err("Unsupported sampler bank catalogue".into());
        }
        banks.selected = lines.next().ok_or("Missing selected bank")?.into();
        banks.entries.clear();
        for line in lines {
            let (id, name) = line.split_once('\t').ok_or("Invalid bank entry")?;
            if !valid_id(id)
                || !valid_name(name)
                || banks
                    .entries
                    .iter()
                    .any(|b| b.id == id || b.name.eq_ignore_ascii_case(name))
            {
                return Err("Invalid or duplicate sampler bank".into());
            }
            banks.entries.push(SampleBank {
                id: id.into(),
                name: name.into(),
            });
        }
        if banks.entries.is_empty()
            || banks.entries.len() > 64
            || !banks.entries.iter().any(|b| b.id == "default")
            || !banks.entries.iter().any(|b| b.id == banks.selected)
        {
            return Err("Invalid sampler bank catalogue".into());
        }
        Ok(banks)
    }
    pub fn path(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|root| {
            if self.selected == "default" {
                root.clone()
            } else {
                root.join("banks").join(&self.selected)
            }
        })
    }
    pub fn create(&mut self, name: String) -> Result<(), String> {
        self.validate_name(&name, None)?;
        if self.entries.len() >= 64 {
            return Err("Maximum 64 sample banks".into());
        }
        let id = (1..)
            .map(|n| format!("bank-{n}"))
            .find(|id| {
                !self.entries.iter().any(|b| b.id == *id)
                    && self
                        .root
                        .as_ref()
                        .is_none_or(|root| !root.join("banks").join(id).exists())
            })
            .unwrap();
        self.entries.push(SampleBank {
            id: id.clone(),
            name,
        });
        self.selected = id;
        Ok(())
    }
    pub fn rename(&mut self, id: &str, name: String) -> Result<(), String> {
        self.validate_name(&name, Some(id))?;
        self.entries
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or("Unknown sample bank")?
            .name = name;
        Ok(())
    }
    fn validate_name(&self, name: &str, except: Option<&str>) -> Result<(), String> {
        if !valid_name(name) {
            return Err("Use a name of 1–48 characters without control characters".into());
        }
        if self
            .entries
            .iter()
            .any(|b| Some(b.id.as_str()) != except && b.name.eq_ignore_ascii_case(name))
        {
            return Err("A bank with that name already exists".into());
        }
        Ok(())
    }
    /// Seed packaged CC0 banks once, leaving the legacy bank and user edits intact.
    pub fn install_starters(&mut self) -> Result<(), String> {
        let Some(root) = self.root.clone() else {
            return Ok(());
        };
        let marker = root.join("bundled-v1.txt");
        if marker.exists() {
            return Ok(());
        }
        let selected = self.selected.clone();
        for (name, samples) in crate::sampler_assets::BANKS {
            let name = if self
                .entries
                .iter()
                .any(|b| b.name.eq_ignore_ascii_case(name))
            {
                format!("{name} (Starter)")
            } else {
                name.to_string()
            };
            self.create(name)?;
            let dir = self.path().unwrap();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            for slot in 0..16 {
                let entry = dir.join(format!("slot-{slot:02}.txt"));
                if let Some((name, bytes)) = samples.get(slot) {
                    let file = format!("sample-{slot:02}-bundled.wav");
                    std::fs::write(dir.join(&file), bytes).map_err(|e| e.to_string())?;
                    std::fs::write(entry, format!("{name}\n{file}")).map_err(|e| e.to_string())?;
                } else {
                    std::fs::write(entry, "").map_err(|e| e.to_string())?;
                }
            }
        }
        self.selected = selected;
        self.save()?;
        std::fs::write(
            root.join("CC0-1.0.txt"),
            include_str!("../../../assets/sampler/frequency303/CC0-1.0.txt"),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("STARTER-SOUNDS.md"),
            include_str!("../../../assets/sampler/frequency303/README.md"),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(marker, "Frequency 303 CC0 starter banks v1\n").map_err(|e| e.to_string())
    }
    pub fn save(&self) -> Result<(), String> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let mut text = format!("1\n{}\n", self.selected);
        for bank in &self.entries {
            text.push_str(&format!("{}\t{}\n", bank.id, bank.name));
        }
        let tmp = root.join("banks-v1.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(tmp, root.join("banks-v1.txt")).map_err(|e| e.to_string())
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= 48
        && !name.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    id == "default"
        || id
            .strip_prefix("bank-")
            .is_some_and(|s| !s.is_empty() && s.len() < 10 && s.bytes().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_safe_and_ids_survive_renaming() {
        let mut banks = Banks::default();
        banks.create("Drums / FX".into()).unwrap();
        let id = banks.selected.clone();
        banks.rename(&id, "Drops".into()).unwrap();
        assert_eq!(banks.selected, id);
        assert!(banks.create("drops".into()).is_err());
        for name in ["", " bad", "bad\nname", "bad\tname"] {
            assert!(banks.create(name.into()).is_err());
        }
        assert!(!valid_id("../outside"));
    }
    #[test]
    fn catalogue_round_trip_preserves_legacy_root_and_selection() {
        let root = std::env::temp_dir().join(format!(
            "fadercraft-banks-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut banks = Banks::open(root.clone()).unwrap();
        assert_eq!(banks.path(), Some(root.clone()));
        banks.create("Sound FX".into()).unwrap();
        banks.save().unwrap();
        let restored = Banks::open(root.clone()).unwrap();
        assert_eq!(restored.entries, banks.entries);
        assert_eq!(restored.selected, banks.selected);
        assert!(restored.path().unwrap().starts_with(root.join("banks")));
        std::fs::remove_file(root.join("banks-v1.txt")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
