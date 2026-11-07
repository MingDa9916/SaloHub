use super::config::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub process_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub button_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub long_press_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub multi_click_bindings: BTreeMap<String, BTreeMap<u8, Action>>,
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 60
            || self.name.contains('\0')
            || self.process_name.len() > 120
            || !self.process_name.to_ascii_lowercase().ends_with(".exe")
            || self.process_name.contains(['\\', '/', ':', '\0'])
        {
            return Err("应用名称或进程名无效，请使用 application.exe 格式".into());
        }
        if self
            .long_press_bindings
            .keys()
            .chain(self.multi_click_bindings.keys())
            .any(|id| !self.button_bindings.contains_key(id))
        {
            return Err("应用的独立触发设置必须先配置对应基础按键".into());
        }
        Ok(())
    }
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridges::hisense::config::HisenseConfig;
    #[test]
    fn app_overrides_are_per_key_and_fixed_disabled_profiles_fall_back() {
        let mut cfg = HisenseConfig::default();
        cfg.long_press_bindings
            .insert("home".into(), Action::SingleKey(66));
        cfg.profiles.insert(
            "browser".into(),
            Profile {
                name: "Browser".into(),
                process_name: "chrome.exe".into(),
                enabled: true,
                button_bindings: [("home".into(), Action::ComboKey(vec![0xa2, 0x54]))].into(),
                long_press_bindings: BTreeMap::new(),
                multi_click_bindings: BTreeMap::new(),
            },
        );
        cfg.validate().unwrap();
        assert_eq!(cfg.profile_for("CHROME.EXE"), "browser");
        assert_eq!(cfg.profile_for("reader.exe"), "default");
        let app = cfg.effective("browser");
        assert_eq!(
            app.button_bindings["home"],
            Action::ComboKey(vec![0xa2, 0x54])
        );
        assert!(!app.long_press_bindings.contains_key("home"));
        assert_eq!(app.button_bindings["ok"], cfg.button_bindings["ok"]);
        cfg.fixed_profile = Some("browser".into());
        assert_eq!(cfg.profile_for("reader.exe"), "browser");
        cfg.profiles.get_mut("browser").unwrap().enabled = false;
        assert_eq!(cfg.profile_for("chrome.exe"), "default");
        cfg.profile_switching_enabled = false;
        assert_eq!(cfg.profile_for("chrome.exe"), "default");
        let doc = super::super::config::MappingsFile::from_config(&cfg);
        let mut imported = HisenseConfig::default();
        doc.apply(&mut imported).unwrap();
        assert_eq!(imported.profiles["browser"].name, "Browser");
        assert!(!imported.profile_switching_enabled);
    }
}
