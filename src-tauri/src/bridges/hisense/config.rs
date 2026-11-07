use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const BUTTONS: &[(&str, &str)] = &[
    ("power", "电源"),
    ("source", "信号源"),
    ("pointer", "指向"),
    ("ai", "AI"),
    ("tv", "电视"),
    ("shortcut", "快捷"),
    ("up", "上"),
    ("left", "左"),
    ("ok", "确认"),
    ("right", "右"),
    ("down", "下"),
    ("back", "返回"),
    ("home", "主页"),
    ("mic", "语音"),
    ("settings", "设置"),
    ("menu", "菜单"),
    ("volume_down", "音量−"),
    ("volume_up", "音量＋"),
    ("pointer_ok", "确认（指向开启）"),
    ("pointer_right", "副键（指向开启）"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum Action {
    SingleKey(u16),
    ComboKey(Vec<u16>),
    MouseClick,
    MouseRightClick,
    MouseMove(MouseMove),
    FocusChatGpt,
    TouchClick,
    PointerConfirm,
    TouchDrag,
    PointerAi,
    ToggleHighlighter,
    Macro(String),
    CycleProfile,
    StopMacros,
    SelectProfile(String),
    TextInput(String),
    LaunchApp(String),
    VoiceInput,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MouseMove {
    pub dx: i32,
    pub dy: i32,
    pub step: u32,
    pub accelerate: bool,
}
impl Action {
    pub fn realtime(&self) -> bool {
        matches!(
            self,
            Self::PointerConfirm | Self::PointerAi | Self::TouchDrag | Self::VoiceInput
        )
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::SingleKey(k) if !super::macro_engine::valid_key(*k) => Err("无效键码".into()),
            Self::ComboKey(k)
                if k.is_empty()
                    || k.len() > 8
                    || k.iter().any(|k| !super::macro_engine::valid_key(*k)) =>
            {
                Err("无效组合键".into())
            }
            Self::TextInput(s) | Self::LaunchApp(s) if s.len() > 4096 || s.contains('\0') => {
                Err("动作内容过长或无效".into())
            }
            Self::MouseMove(m)
                if !(-1..=1).contains(&m.dx)
                    || !(-1..=1).contains(&m.dy)
                    || m.dx.abs() + m.dy.abs() != 1
                    || !(1..=100).contains(&m.step) =>
            {
                Err("鼠标方向或步幅无效".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HisenseConfig {
    pub auto_connect: bool,
    pub pointer_enabled: bool,
    pub touch_indicator_enabled: bool,
    pub indicator_color: String,
    pub indicator_pressed_color: String,
    pub highlight_trail_enabled: bool,
    pub highlight_trail_color: String,
    pub highlight_trail_width_px: u32,
    pub highlight_trail_duration_ms: u64,
    pub highlight_trail_opacity_percent: u32,
    pub highlight_trail_glow_percent: u32,
    pub pointer_hold_ms: u64,
    #[serde(default)]
    pub pointer_gesture_revision: u8,
    pub sensitivity: f64,
    pub adaptive_sensitivity: bool,
    pub precision_ratio: f64,
    pub smoothing_ms: f64,
    pub deadzone_pixels: f64,
    pub macros: BTreeMap<String, super::macro_engine::Definition>,
    pub profiles: BTreeMap<String, super::profiles::Profile>,
    pub profile_switching_enabled: bool,
    pub fixed_profile: Option<String>,
    pub button_bindings: BTreeMap<String, Action>,
    pub long_press_bindings: BTreeMap<String, Action>,
    pub multi_click_bindings: BTreeMap<String, BTreeMap<u8, Action>>,
    pub multi_click_interval_ms: u64,
    /// Device-only source token -> physical button. No machine/serial IDs.
    pub button_sources: BTreeMap<String, String>,
    pub voice_keys: Vec<u16>,
    pub voice_profile: String,
    pub voice_mode: String,
    pub microphone_name: Option<String>,
    pub microphone_gain_db: f32,
    #[serde(default)]
    pub confirm_touch_revision: u8,
}

impl Default for HisenseConfig {
    fn default() -> Self {
        let mut bindings: BTreeMap<String, Action> = BUTTONS
            .iter()
            .map(|(id, _)| ((*id).into(), Action::None))
            .collect();
        for (id, vk) in [
            ("up", 0x26),
            ("down", 0x28),
            ("left", 0x25),
            ("right", 0x27),
            ("ok", 0x20),
            ("back", 0x1b),
            ("home", 0x5b),
            ("menu", 0x5d),
            ("volume_up", 0xaf),
            ("volume_down", 0xae),
        ] {
            bindings.insert(id.into(), Action::SingleKey(vk));
        }
        bindings.insert("mic".into(), Action::VoiceInput);
        bindings.insert("ok".into(), Action::PointerConfirm);
        bindings.insert("pointer_ok".into(), Action::PointerConfirm);
        bindings.insert("ai".into(), Action::PointerAi);
        bindings.insert("pointer_right".into(), Action::MouseRightClick);
        Self {
            macros: BTreeMap::new(),
            profiles: BTreeMap::new(),
            profile_switching_enabled: true,
            fixed_profile: None,
            auto_connect: true,
            pointer_enabled: true,
            touch_indicator_enabled: true,
            indicator_color: "#ffffff".into(),
            indicator_pressed_color: "#3b82f6".into(),
            highlight_trail_enabled: true,
            highlight_trail_color: "#22d3ee".into(),
            highlight_trail_width_px: 6,
            highlight_trail_duration_ms: 650,
            highlight_trail_opacity_percent: 66,
            highlight_trail_glow_percent: 50,
            pointer_hold_ms: 350,
            pointer_gesture_revision: 2,
            sensitivity: 2.,
            adaptive_sensitivity: true,
            precision_ratio: 0.25,
            smoothing_ms: 18.,
            deadzone_pixels: 1.5,
            button_bindings: bindings,
            long_press_bindings: BTreeMap::new(),
            multi_click_bindings: BTreeMap::new(),
            multi_click_interval_ms: 300,
            button_sources: [
                ("key:52", "up"),
                ("key:51", "down"),
                ("key:50", "left"),
                ("key:4f", "right"),
                ("key:28", "ok"),
                ("key:58", "ok"),
                ("key:29", "back"),
                ("key:f1", "back"),
                ("key:e3", "home"),
                ("key:65", "menu"),
                ("key:80", "volume_up"),
                ("key:81", "volume_down"),
                ("key:94", "shortcut"),
                ("pen:tip", "pointer_ok"),
                ("pen:barrel", "pointer_right"),
            ]
            .into_iter()
            .map(|(s, b)| (s.into(), b.into()))
            .collect(),
            voice_keys: vec![0xa2, 0xa0, 0x44],
            voice_profile: "codex".into(),
            voice_mode: "Hold".into(),
            microphone_name: None,
            microphone_gain_db: 0.,
            confirm_touch_revision: 1,
        }
    }
}

impl HisenseConfig {
    pub fn reset_mappings(
        &mut self,
        profile: Option<&str>,
        button: Option<&str>,
    ) -> Result<(), String> {
        if button.is_some_and(|key| !BUTTONS.iter().any(|(id, _)| *id == key)) {
            return Err("未知按键".into());
        }
        if let Some(id) = profile.filter(|id| *id != "default") {
            let p = self.profiles.get_mut(id).ok_or("应用配置不存在")?;
            if let Some(key) = button {
                p.button_bindings.remove(key);
                p.long_press_bindings.remove(key);
                p.multi_click_bindings.remove(key);
            } else {
                p.button_bindings.clear();
                p.long_press_bindings.clear();
                p.multi_click_bindings.clear();
            }
        } else if let Some(key) = button {
            self.button_bindings
                .insert(key.into(), Self::default().button_bindings[key].clone());
            self.long_press_bindings.remove(key);
            self.multi_click_bindings.remove(key);
        } else {
            self.button_bindings = Self::default().button_bindings;
            self.long_press_bindings.clear();
            self.multi_click_bindings.clear();
        }
        Ok(())
    }
    fn repair_input_sources(&mut self) -> bool {
        let before = self.button_sources.len();
        self.button_sources
            .retain(|source, _| !super::protocol::telemetry_source(source));
        let removed = self.button_sources.len() != before;
        if !self.button_sources.contains_key("key:94") {
            self.button_sources
                .insert("key:94".into(), "shortcut".into());
            return true;
        }
        removed
    }
    pub fn indicator_style(&self) -> super::visual::Style {
        super::visual::Style {
            color: super::visual::Rgb::parse(&self.indicator_color)
                .unwrap_or(super::visual::Style::default().color),
            pressed_color: super::visual::Rgb::parse(&self.indicator_pressed_color)
                .unwrap_or(super::visual::Style::default().pressed_color),
            trail: super::visual::TrailOptions {
                width_px: self.highlight_trail_width_px,
                duration_ms: self.highlight_trail_duration_ms,
                opacity_percent: self.highlight_trail_opacity_percent,
                glow_percent: self.highlight_trail_glow_percent,
            },
            trail_enabled: self.highlight_trail_enabled,
            trail_color: super::visual::Rgb::parse(&self.highlight_trail_color)
                .unwrap_or(super::visual::Style::default().trail_color),
        }
    }
    pub fn realtime(&self, action: &Action) -> bool {
        action.realtime()
            || matches!(action, Action::Macro(id) if self.macros.get(id).is_some_and(|m|m.realtime()))
    }
    pub fn profile_for(&self, process: &str) -> String {
        if !self.profile_switching_enabled {
            return "default".into();
        }
        if let Some(id) = &self.fixed_profile {
            return if id == "default" || self.profiles.get(id).is_some_and(|p| p.enabled) {
                id.clone()
            } else {
                "default".into()
            };
        }
        self.profiles
            .iter()
            .find(|(_, p)| p.enabled && p.process_name.to_lowercase() == process.to_lowercase())
            .map(|(id, _)| id.clone())
            .unwrap_or_else(|| "default".into())
    }
    pub fn effective(&self, id: &str) -> Self {
        let mut next = self.clone();
        if let Some(p) = self.profiles.get(id) {
            for (key, action) in &p.button_bindings {
                next.button_bindings.insert(key.clone(), action.clone());
                next.long_press_bindings.remove(key);
                next.multi_click_bindings.remove(key);
                if let Some(a) = p.long_press_bindings.get(key) {
                    next.long_press_bindings.insert(key.clone(), a.clone());
                }
                if let Some(a) = p.multi_click_bindings.get(key) {
                    next.multi_click_bindings.insert(key.clone(), a.clone());
                }
            }
        }
        next
    }
    fn validate_action(&self, action: &Action) -> Result<(), String> {
        action.validate()?;
        match action {
            Action::Macro(id) if !self.macros.contains_key(id) => Err("映射引用的宏不存在".into()),
            Action::SelectProfile(id)
                if !id.is_empty() && id != "default" && !self.profiles.contains_key(id) =>
            {
                Err("映射引用的应用配置不存在".into())
            }
            _ => Ok(()),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_internal(true)
    }
    fn validate_internal(&self, check_profiles: bool) -> Result<(), String> {
        if self.macros.len() > 64 || self.profiles.len() > 64 {
            return Err("宏和应用配置分别最多 64 个".into());
        }
        for (id, m) in &self.macros {
            if !super::profiles::valid_id(id) {
                return Err("宏编号无效".into());
            }
            m.validate()?;
        }
        if let Some(id) = &self.fixed_profile {
            if id != "default" && !self.profiles.contains_key(id) {
                return Err("固定配置不存在".into());
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for (id, p) in &self.profiles {
            if id == "default" || !super::profiles::valid_id(id) {
                return Err("配置编号无效".into());
            }
            p.validate()?;
            if p.enabled && !names.insert(p.process_name.to_lowercase()) {
                return Err("同一进程只能启用一个自动配置".into());
            }
            if check_profiles {
                self.effective(id).validate_internal(false)?;
            }
        }
        if !(2..=20).contains(&self.highlight_trail_width_px)
            || !(150..=2000).contains(&self.highlight_trail_duration_ms)
            || !(10..=100).contains(&self.highlight_trail_opacity_percent)
            || self.highlight_trail_glow_percent > 100
        {
            return Err("荧光笔参数超出范围".into());
        }
        if super::visual::Rgb::parse(&self.indicator_color).is_none()
            || super::visual::Rgb::parse(&self.indicator_pressed_color).is_none()
            || super::visual::Rgb::parse(&self.highlight_trail_color).is_none()
        {
            return Err("圆点颜色需为 #RRGGBB 格式".into());
        }
        if !self.precision_ratio.is_finite() || !(0.1..=0.75).contains(&self.precision_ratio) {
            return Err("慢移倍率需为 10%–75%".into());
        }
        if !(150..=1000).contains(&self.pointer_hold_ms) {
            return Err("确认长按阈值需为 150–1000 毫秒".into());
        }
        if !self.sensitivity.is_finite()
            || !(0.5..=4.).contains(&self.sensitivity)
            || !self.smoothing_ms.is_finite()
            || !(0.0..=80.).contains(&self.smoothing_ms)
            || !self.deadzone_pixels.is_finite()
            || !(0.0..=12.).contains(&self.deadzone_pixels)
        {
            return Err("灵敏度或防抖参数超出范围".into());
        }
        if self.voice_keys.is_empty()
            || self.voice_keys.len() > 8
            || self.voice_keys.iter().any(|k| *k == 0 || *k > 0xfe)
            || !matches!(self.voice_mode.as_str(), "Hold" | "Toggle")
            || !self.microphone_gain_db.is_finite()
            || !(0.0..=24.).contains(&self.microphone_gain_db)
        {
            return Err("语音快捷键或增益无效".into());
        }
        if !(150..=800).contains(&self.multi_click_interval_ms) {
            return Err("连击间隔需为 150–800 毫秒".into());
        }
        for (id, action) in self
            .button_bindings
            .iter()
            .chain(self.long_press_bindings.iter())
        {
            if !BUTTONS.iter().any(|(b, _)| b == id) {
                return Err("未知遥控器按键".into());
            }
            self.validate_action(action)?;
        }
        for (id, slots) in &self.multi_click_bindings {
            if !BUTTONS.iter().any(|(b, _)| b == id) {
                return Err("未知遥控器按键".into());
            }
            for (count, action) in slots {
                if !(2..=4).contains(count) {
                    return Err("仅支持 2、3、4 连击".into());
                }
                self.validate_action(action)?;
                if self.realtime(action) {
                    return Err("连击不能执行需持续按住的语音、拖动或 UWB 模式动作".into());
                }
            }
        }
        for (id, action) in &self.long_press_bindings {
            if matches!(action, Action::PointerConfirm | Action::PointerAi) {
                return Err("UWB 确认和模式切换请设为基础动作".into());
            }
            if !matches!(action, Action::None)
                && self
                    .button_bindings
                    .get(id)
                    .is_some_and(|a| self.realtime(a))
            {
                return Err("实时语音、拖动和 UWB 按键不能同时配置独立长按".into());
            }
        }
        for (id, slots) in &self.multi_click_bindings {
            if slots.values().any(|a| !matches!(a, Action::None))
                && self
                    .button_bindings
                    .get(id)
                    .is_some_and(|a| self.realtime(a))
            {
                return Err("实时语音、拖动和 UWB 按键不能同时配置连击".into());
            }
        }
        for (source, button) in &self.button_sources {
            if !super::protocol::valid_source(source) || !BUTTONS.iter().any(|(b, _)| b == button) {
                return Err("按键学习数据无效".into());
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut cfg: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if cfg.confirm_touch_revision < 1 {
            cfg.button_bindings.insert("ok".into(), Action::TouchClick);
            cfg.button_bindings
                .insert("pointer_ok".into(), Action::TouchClick);
            cfg.confirm_touch_revision = 1;
            cfg.save(path)?;
        }
        if cfg.pointer_gesture_revision < 1 {
            cfg.button_bindings
                .insert("ok".into(), Action::PointerConfirm);
            cfg.button_bindings
                .insert("pointer_ok".into(), Action::PointerConfirm);
            cfg.button_bindings.insert("ai".into(), Action::TouchDrag);
            cfg.pointer_gesture_revision = 1;
            cfg.save(path)?;
        }
        if cfg.pointer_gesture_revision < 2 {
            cfg.button_bindings.insert("ai".into(), Action::PointerAi);
            cfg.pointer_gesture_revision = 2;
            cfg.save(path)?;
        }
        if cfg.repair_input_sources() {
            cfg.save(path)?;
        }
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        std::fs::rename(tmp, path).map_err(|e| e.to_string())
    }
}

/// Same mapping-only document model as Xiaomi, with an explicit device format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingsFile {
    pub format: String,
    pub schema_version: u32,
    #[serde(default)]
    pub macros: BTreeMap<String, super::macro_engine::Definition>,
    #[serde(default)]
    pub profiles: BTreeMap<String, super::profiles::Profile>,
    #[serde(default = "default_profile_switching")]
    pub profile_switching_enabled: bool,
    #[serde(default)]
    pub fixed_profile: Option<String>,
    pub button_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub long_press_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub multi_click_bindings: BTreeMap<String, BTreeMap<u8, Action>>,
    #[serde(default = "default_interval")]
    pub multi_click_interval_ms: u64,
}
fn default_profile_switching() -> bool {
    true
}
fn default_interval() -> u64 {
    300
}
impl MappingsFile {
    pub fn from_config(cfg: &HisenseConfig) -> Self {
        Self {
            macros: cfg.macros.clone(),
            profiles: cfg.profiles.clone(),
            profile_switching_enabled: cfg.profile_switching_enabled,
            fixed_profile: cfg.fixed_profile.clone(),
            format: "nexus-prime-hisense-mappings".into(),
            schema_version: 1,
            button_bindings: cfg.button_bindings.clone(),
            long_press_bindings: cfg.long_press_bindings.clone(),
            multi_click_bindings: cfg.multi_click_bindings.clone(),
            multi_click_interval_ms: cfg.multi_click_interval_ms,
        }
    }
    pub fn apply(self, cfg: &mut HisenseConfig) -> Result<(), String> {
        if self.format != "nexus-prime-hisense-mappings" || self.schema_version != 1 {
            return Err("不支持的海信映射格式或版本".into());
        }
        let mut next = cfg.clone();
        next.macros = self.macros;
        next.profiles = self.profiles;
        next.profile_switching_enabled = self.profile_switching_enabled;
        next.fixed_profile = self.fixed_profile;
        if next
            .fixed_profile
            .as_ref()
            .is_some_and(|id| id != "default" && !next.profiles.contains_key(id))
        {
            next.fixed_profile = None;
        }
        next.button_bindings = BUTTONS
            .iter()
            .map(|(id, _)| ((*id).into(), Action::None))
            .collect();
        next.button_bindings.extend(self.button_bindings);
        next.long_press_bindings = self.long_press_bindings;
        next.multi_click_bindings = self.multi_click_bindings;
        next.multi_click_interval_ms = self.multi_click_interval_ms;
        next.validate()?;
        *cfg = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_one_button_preserves_other_settings_and_app_overrides() {
        let mut cfg = HisenseConfig::default();
        cfg.sensitivity = 3.;
        cfg.button_bindings
            .insert("up".into(), Action::SingleKey(0x21));
        cfg.button_bindings
            .insert("down".into(), Action::SingleKey(65));
        cfg.long_press_bindings
            .insert("up".into(), Action::SingleKey(66));
        cfg.multi_click_bindings
            .insert("up".into(), [(2, Action::SingleKey(67))].into());
        let p: super::super::profiles::Profile = serde_json::from_value(serde_json::json!({
            "name":"Chrome","process_name":"chrome.exe","enabled":true,
            "button_bindings":{"up":{"type":"SingleKey","value":68}}
        }))
        .unwrap();
        cfg.profiles.insert("chrome".into(), p);
        let sources = cfg.button_sources.clone();
        cfg.reset_mappings(Some("default"), Some("up")).unwrap();
        assert_eq!(cfg.button_bindings["up"], Action::SingleKey(0x26));
        assert_eq!(cfg.button_bindings["down"], Action::SingleKey(65));
        assert!(!cfg.long_press_bindings.contains_key("up"));
        assert!(!cfg.multi_click_bindings.contains_key("up"));
        assert_eq!(
            cfg.profiles["chrome"].button_bindings["up"],
            Action::SingleKey(68)
        );
        assert_eq!(cfg.sensitivity, 3.);
        assert_eq!(cfg.button_sources, sources);
        cfg.profiles
            .get_mut("chrome")
            .unwrap()
            .long_press_bindings
            .insert("up".into(), Action::SingleKey(69));
        cfg.reset_mappings(Some("chrome"), Some("up")).unwrap();
        assert!(cfg.profiles["chrome"].button_bindings.is_empty());
        assert!(cfg.profiles["chrome"].long_press_bindings.is_empty());
        assert_eq!(
            cfg.effective("chrome").button_bindings["up"],
            Action::SingleKey(0x26)
        );
        let before = serde_json::to_value(&cfg).unwrap();
        assert!(cfg.reset_mappings(None, Some("missing")).is_err());
        assert!(cfg.reset_mappings(Some("missing"), Some("up")).is_err());
        assert_eq!(serde_json::to_value(&cfg).unwrap(), before);
    }
    #[test]
    fn macros_roundtrip_and_realtime_bindings_are_validated() {
        let mut cfg = HisenseConfig::default();
        cfg.button_bindings
            .insert("menu".into(), Action::Macro("sample".into()));
        assert!(cfg.validate().is_err());
        let def:super::super::macro_engine::Definition=serde_json::from_str(r##"{"name":"Sample","mode":"Once","color":"#3478f6","use_default_delay":true,"default_delay_ms":50,"show_key_transitions":true,"press":[{"type":"KeyDown","value":269},{"type":"KeyUp","value":269}]}"##).unwrap();
        cfg.macros.insert("sample".into(), def.clone());
        cfg.validate().unwrap();
        cfg.multi_click_bindings
            .insert("home".into(), [(2, Action::Macro("sample".into()))].into());
        cfg.validate().unwrap();
        let document = MappingsFile::from_config(&cfg);
        let json = serde_json::to_string(&document).unwrap();
        let mut target = HisenseConfig::default();
        serde_json::from_str::<MappingsFile>(&json)
            .unwrap()
            .apply(&mut target)
            .unwrap();
        assert_eq!(target.macros["sample"], def);
        assert_eq!(
            target.button_bindings["menu"],
            Action::Macro("sample".into())
        );
        cfg.macros.get_mut("sample").unwrap().mode = super::super::macro_engine::Mode::Hold;
        assert!(cfg.validate().is_err());
        cfg.multi_click_bindings.clear();
        cfg.validate().unwrap();
    }
    #[test]
    fn old_json_keeps_user_actions_and_gains_mapping_defaults() {
        let mut value = serde_json::to_value(HisenseConfig::default()).unwrap();
        for field in [
            "long_press_bindings",
            "multi_click_bindings",
            "multi_click_interval_ms",
            "indicator_color",
            "indicator_pressed_color",
            "highlight_trail_enabled",
            "highlight_trail_color",
            "highlight_trail_width_px",
            "highlight_trail_duration_ms",
            "highlight_trail_opacity_percent",
            "highlight_trail_glow_percent",
        ] {
            value.as_object_mut().unwrap().remove(field);
        }
        value["sensitivity"] = serde_json::json!(2.7);
        value["button_bindings"]["menu"] = serde_json::json!({"type":"SingleKey", "value":65});
        let c: HisenseConfig = serde_json::from_value(value).unwrap();
        assert_eq!(c.sensitivity, 2.7);
        assert_eq!(c.button_bindings["menu"], Action::SingleKey(65));
        assert_eq!(c.multi_click_interval_ms, 300);
        assert!(c.long_press_bindings.is_empty() && c.multi_click_bindings.is_empty());
        c.validate().unwrap();
    }
    #[test]
    fn mappings_round_trip_all_triggers_preserving_environment_and_learning() {
        let mut source = HisenseConfig::default();
        source.long_press_bindings.insert(
            "menu".into(),
            Action::MouseMove(MouseMove {
                dx: 1,
                dy: 0,
                step: 12,
                accelerate: true,
            }),
        );
        source.multi_click_bindings.insert(
            "home".into(),
            [
                (2, Action::FocusChatGpt),
                (3, Action::TextInput("test".into())),
                (4, Action::ComboKey(vec![0xa2, 0x41])),
            ]
            .into(),
        );
        source.multi_click_interval_ms = 450;
        let text = serde_json::to_string(&MappingsFile::from_config(&source)).unwrap();
        assert!(!text.contains("button_sources") && !text.contains("microphone_name"));
        let mut target = HisenseConfig::default();
        target.sensitivity = 3.2;
        target.voice_profile = "qianwen".into();
        target.voice_keys = vec![0xa5];
        target.button_sources.insert("key:3e".into(), "menu".into());
        serde_json::from_str::<MappingsFile>(&text)
            .unwrap()
            .apply(&mut target)
            .unwrap();
        assert_eq!(target.button_bindings, source.button_bindings);
        assert_eq!(target.long_press_bindings, source.long_press_bindings);
        assert_eq!(target.multi_click_bindings, source.multi_click_bindings);
        assert_eq!(target.multi_click_interval_ms, 450);
        assert_eq!(target.sensitivity, 3.2);
        assert_eq!(target.voice_keys, vec![0xa5]);
        assert_eq!(target.button_sources["key:3e"], "menu");
    }
    #[test]
    fn malformed_or_conflicting_import_never_changes_live_configuration() {
        let mut target = HisenseConfig::default();
        let before = serde_json::to_value(&target).unwrap();
        for variant in 0..5 {
            let mut document = MappingsFile::from_config(&target);
            match variant {
                0 => document.format = "nexus-prime-xiaomi-mappings".into(),
                1 => document.schema_version = 99,
                2 => document.multi_click_interval_ms = 1,
                3 => {
                    document
                        .multi_click_bindings
                        .insert("home".into(), [(5, Action::SingleKey(65))].into());
                }
                _ => {
                    document
                        .long_press_bindings
                        .insert("ok".into(), Action::SingleKey(65));
                }
            }
            assert!(document.apply(&mut target).is_err());
            assert_eq!(serde_json::to_value(&target).unwrap(), before);
        }
        let mut json = serde_json::to_value(MappingsFile::from_config(&target)).unwrap();
        json["button_sources"] = serde_json::json!({});
        assert!(serde_json::from_value::<MappingsFile>(json).is_err());
    }
    #[test]
    fn rejects_impossible_mouse_and_realtime_click_actions() {
        for (dx, dy, step) in [(i32::MIN, 0, 1), (1, 1, 1), (0, 0, 1), (1, 0, 101)] {
            let mut c = HisenseConfig::default();
            c.long_press_bindings.insert(
                "home".into(),
                Action::MouseMove(MouseMove {
                    dx,
                    dy,
                    step,
                    accelerate: false,
                }),
            );
            assert!(c.validate().is_err());
        }
        let mut c = HisenseConfig::default();
        c.multi_click_bindings
            .insert("home".into(), [(2, Action::VoiceInput)].into());
        assert!(c.validate().is_err());
        c.multi_click_bindings.clear();
        c.long_press_bindings
            .insert("home".into(), Action::VoiceInput);
        c.validate().unwrap();
    }
    #[test]
    fn visual_preferences_validate_and_old_config_gets_defaults() {
        let mut json = serde_json::to_value(HisenseConfig::default()).unwrap();
        for key in [
            "indicator_color",
            "indicator_pressed_color",
            "highlight_trail_enabled",
            "highlight_trail_color",
            "highlight_trail_width_px",
            "highlight_trail_duration_ms",
            "highlight_trail_opacity_percent",
            "highlight_trail_glow_percent",
        ] {
            json.as_object_mut().unwrap().remove(key);
        }
        let mut cfg: HisenseConfig = serde_json::from_value(json).unwrap();
        assert_eq!(
            cfg.indicator_style(),
            super::super::visual::Style::default()
        );
        cfg.indicator_color = "#ff9900".into();
        cfg.highlight_trail_enabled = true;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.indicator_style().color,
            super::super::visual::Rgb(255, 153, 0)
        );
        cfg.highlight_trail_color = "#oops".into();
        assert!(cfg.validate().is_err());
    }
    #[test]
    fn highlighter_defaults_and_limits_preserve_explicit_disable() {
        let cfg: HisenseConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.highlight_trail_enabled);
        let cfg: HisenseConfig =
            serde_json::from_str(r#"{"highlight_trail_enabled":false}"#).unwrap();
        assert!(!cfg.highlight_trail_enabled);
        cfg.validate().unwrap();
        for (width, duration, opacity, glow) in [
            (1, 650, 66, 50),
            (6, 2001, 66, 50),
            (6, 650, 9, 50),
            (6, 650, 66, 101),
        ] {
            let mut cfg = HisenseConfig::default();
            cfg.highlight_trail_width_px = width;
            cfg.highlight_trail_duration_ms = duration;
            cfg.highlight_trail_opacity_percent = opacity;
            cfg.highlight_trail_glow_percent = glow;
            assert!(cfg.validate().is_err());
        }
        assert!(!Action::ToggleHighlighter.realtime());
    }
    #[test]
    fn repairs_false_learning_without_overwriting_user_actions_or_real_sources() {
        let mut cfg = HisenseConfig::default();
        cfg.button_bindings
            .insert("shortcut".into(), Action::PointerAi);
        cfg.button_sources.remove("key:94");
        cfg.button_sources
            .insert("raw:f08002b80100004af6".into(), "shortcut".into());
        let actions = cfg.button_bindings.clone();
        assert!(cfg.repair_input_sources());
        assert_eq!(cfg.button_sources["key:94"], "shortcut");
        assert!(!cfg
            .button_sources
            .keys()
            .any(|s| super::super::protocol::telemetry_source(s)));
        assert_eq!(cfg.button_bindings, actions);
        assert!(!cfg.repair_input_sources());
        cfg.button_sources.insert("key:94".into(), "menu".into());
        assert!(!cfg.repair_input_sources());
        assert_eq!(cfg.button_sources["key:94"], "menu");
    }
    #[test]
    fn every_button_can_be_voice_without_changing_other_bindings() {
        let mut c = HisenseConfig::default();
        for (id, _) in BUTTONS {
            c.button_bindings.insert((*id).into(), Action::VoiceInput);
        }
        assert!(c.validate().is_ok());
        assert_eq!(c.button_bindings.len(), BUTTONS.len());
    }
    #[test]
    fn rejects_invalid_configuration() {
        let mut c = HisenseConfig::default();
        c.deadzone_pixels = f64::NAN;
        assert!(c.validate().is_err());
        c.deadzone_pixels = 1.;
        c.button_sources.insert("pointer:3f".into(), "ok".into());
        assert!(c.validate().is_err());
    }
}
