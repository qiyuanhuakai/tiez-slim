use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub name: String,
    pub combo: String,
}

#[derive(Debug, Default)]
pub struct HotkeyManager {
    registered: HashMap<String, HotkeyEntry>,
    combo_index: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    ComboConflict {
        combo: String,
        existing_name: String,
    },
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::ComboConflict {
                combo,
                existing_name,
            } => write!(
                f,
                "hotkey combo '{combo}' is already registered by '{existing_name}'"
            ),
        }
    }
}

impl std::error::Error for HotkeyError {}

impl HotkeyManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &str, combo: &str) -> Result<(), HotkeyError> {
        let combo = combo.trim().to_string();
        if combo.is_empty() {
            self.unregister(name);
            self.registered.insert(
                name.to_string(),
                HotkeyEntry {
                    name: name.to_string(),
                    combo: combo.clone(),
                },
            );
            return Ok(());
        }

        if let Some(old) = self.registered.get(name) {
            if old.combo == combo {
                return Ok(());
            }
            if !old.combo.is_empty() {
                self.combo_index.remove(&old.combo);
            }
        }

        if let Some(existing) = self.combo_index.get(&combo) {
            if existing != name {
                return Err(HotkeyError::ComboConflict {
                    combo,
                    existing_name: existing.clone(),
                });
            }
        }

        self.combo_index.insert(combo.clone(), name.to_string());
        self.registered.insert(
            name.to_string(),
            HotkeyEntry {
                name: name.to_string(),
                combo,
            },
        );
        Ok(())
    }

    pub fn unregister(&mut self, name: &str) {
        if let Some(entry) = self.registered.remove(name) {
            if !entry.combo.is_empty() {
                self.combo_index.remove(&entry.combo);
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.registered.get(name).map(|e| e.combo.as_str())
    }

    pub fn entries(&self) -> Vec<&HotkeyEntry> {
        self.registered.values().collect()
    }

    pub fn conflict_for(&self, combo: &str) -> Option<&str> {
        self.combo_index.get(combo.trim()).map(|name| name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut mgr = HotkeyManager::new();
        mgr.register("private_mode", "Ctrl+Alt+P").unwrap();
        assert_eq!(mgr.get("private_mode"), Some("Ctrl+Alt+P"));
    }

    #[test]
    fn conflict_detection() {
        let mut mgr = HotkeyManager::new();
        mgr.register("private_mode", "Ctrl+Alt+P").unwrap();
        let err = mgr.register("other_action", "Ctrl+Alt+P");
        assert!(matches!(err, Err(HotkeyError::ComboConflict { .. })));
    }

    #[test]
    fn reregister_same_name_changes_combo() {
        let mut mgr = HotkeyManager::new();
        mgr.register("private_mode", "Ctrl+Alt+P").unwrap();
        mgr.register("private_mode", "Ctrl+Alt+X").unwrap();
        assert_eq!(mgr.get("private_mode"), Some("Ctrl+Alt+X"));
        assert!(mgr.conflict_for("Ctrl+Alt+P").is_none());
    }

    #[test]
    fn unregister_frees_combo() {
        let mut mgr = HotkeyManager::new();
        mgr.register("private_mode", "Ctrl+Alt+P").unwrap();
        mgr.unregister("private_mode");
        assert!(mgr.get("private_mode").is_none());
        mgr.register("other", "Ctrl+Alt+P").unwrap();
        assert_eq!(mgr.get("other"), Some("Ctrl+Alt+P"));
    }

    #[test]
    fn empty_combo_skips_conflict() {
        let mut mgr = HotkeyManager::new();
        mgr.register("a", "").unwrap();
        mgr.register("b", "").unwrap();
        assert_eq!(mgr.get("a"), Some(""));
        assert_eq!(mgr.get("b"), Some(""));
    }

    #[test]
    fn conflict_for_returns_name() {
        let mut mgr = HotkeyManager::new();
        mgr.register("private_mode", "Ctrl+Alt+P").unwrap();
        assert_eq!(mgr.conflict_for("Ctrl+Alt+P"), Some("private_mode"));
        assert_eq!(mgr.conflict_for("Ctrl+Alt+X"), None);
    }
}
