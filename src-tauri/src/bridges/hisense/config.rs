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
            Self::SingleKey(k) if *k == 0 || *k > 0xfe => Err("无效键码".into()),
            Self::ComboKey(k)
                if k.is_empty() || k.len() > 8 || k.iter().any(|k| *k == 0 || *k > 0xfe) =>
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
    pub pointer_hold_ms: u64,
    #[serde(default)]
    pub pointer_gesture_revision: u8,
    pub sensitivity: f64,
    pub adaptive_sensitivity: bool,
    pub precision_ratio: f64,
    pub smoothing_ms: f64,
    pub deadzone_pixels: f64,
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
            auto_connect: true,
            pointer_enabled: true,
            touch_indicator_enabled: true,
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
    pub fn validate(&self) -> Result<(), String> {
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
            action.validate()?;
        }
        for (id, slots) in &self.multi_click_bindings {
            if !BUTTONS.iter().any(|(b, _)| b == id) {
                return Err("未知遥控器按键".into());
            }
            for (count, action) in slots {
                if !(2..=4).contains(count) {
                    return Err("仅支持 2、3、4 连击".into());
                }
                action.validate()?;
                if action.realtime() {
                    return Err("连击不能执行需持续按住的语音、拖动或 UWB 模式动作".into());
                }
            }
        }
        for (id, action) in &self.long_press_bindings {
            if matches!(action, Action::PointerConfirm | Action::PointerAi) {
                return Err("UWB 确认和模式切换请设为基础动作".into());
            }
            if !matches!(action, Action::None)
                && self.button_bindings.get(id).is_some_and(Action::realtime)
            {
                return Err("实时语音、拖动和 UWB 按键不能同时配置独立长按".into());
            }
        }
        for (id, slots) in &self.multi_click_bindings {
            if slots.values().any(|a| !matches!(a, Action::None))
                && self.button_bindings.get(id).is_some_and(Action::realtime)
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
    pub button_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub long_press_bindings: BTreeMap<String, Action>,
    #[serde(default)]
    pub multi_click_bindings: BTreeMap<String, BTreeMap<u8, Action>>,
    #[serde(default = "default_interval")]
    pub multi_click_interval_ms: u64,
}
fn default_interval() -> u64 {
    300
}
impl MappingsFile {
    pub fn from_config(cfg: &HisenseConfig) -> Self {
        Self {
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
    fn old_json_keeps_user_actions_and_gains_mapping_defaults() {
        let mut value = serde_json::to_value(HisenseConfig::default()).unwrap();
        for field in [
            "long_press_bindings",
            "multi_click_bindings",
            "multi_click_interval_ms",
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
