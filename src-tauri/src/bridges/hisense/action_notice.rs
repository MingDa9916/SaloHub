use super::{
    config::{Action, HisenseConfig, BUTTONS},
    gesture::Mode,
};

pub fn caption(button: Option<&str>, trigger: &str) -> String {
    let name = BUTTONS
        .iter()
        .find(|(id, _)| Some(*id) == button)
        .map(|(_, name)| *name)
        .unwrap_or("遥控按键");
    let trigger = match trigger {
        "long" => "长按",
        "click:2" => "双击",
        "click:3" => "三击",
        "click:4" => "四连击",
        _ => "单击",
    };
    format!("{name} · {trigger}")
}

pub fn describe(action: &Action, cfg: &HisenseConfig, mode: Mode) -> String {
    match action {
        Action::SingleKey(key) => key_label(*key),
        Action::ComboKey(keys) => keys
            .iter()
            .map(|k| key_label(*k))
            .collect::<Vec<_>>()
            .join(" + "),
        Action::Macro(id) => format!(
            "宏 · {}",
            cfg.macros
                .get(id)
                .map(|m| m.name.as_str())
                .unwrap_or("未找到")
        ),
        Action::SelectProfile(id) if id.is_empty() => "按应用自动切换".into(),
        Action::SelectProfile(id) => format!(
            "配置 · {}",
            if id == "default" {
                "桌面默认"
            } else {
                cfg.profiles
                    .get(id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("未找到")
            }
        ),
        Action::MouseMove(m) => format!(
            "鼠标{} · {} px",
            if m.dx < 0 {
                "向左"
            } else if m.dx > 0 {
                "向右"
            } else if m.dy < 0 {
                "向上"
            } else {
                "向下"
            },
            m.step
        ),
        Action::PointerConfirm => match mode {
            Mode::Touch => "触摸点按／按住拖动".into(),
            Mode::Move => "短按点按／长按移动圆点".into(),
        },
        _ => match action {
            Action::MouseClick => "鼠标左键",
            Action::MouseRightClick => "鼠标右键",
            Action::FocusChatGpt => "打开并聚焦 ChatGPT",
            Action::TouchClick => "触屏点按",
            Action::TouchDrag => "按住触摸拖动",
            Action::PointerAi => "循环切换两种模式",
            Action::ToggleHighlighter => "荧光笔开关",
            Action::CycleProfile => "循环切换配置文件",
            Action::StopMacros => "停止所有宏",
            Action::VoiceInput => "语音输入",
            Action::TextInput(_) => "输入文字",
            Action::LaunchApp(_) => "启动应用",
            Action::None => "不执行动作",
            _ => unreachable!(),
        }
        .into(),
    }
}

fn key_label(key: u16) -> String {
    match key {
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(key.into()).unwrap().to_string(),
        0x60..=0x69 => format!("Num {}", key - 0x60),
        0x70..=0x87 => format!("F{}", key - 0x6f),
        _ => {
            let name = match key {
                0x08 => "Backspace",
                0x09 => "Tab",
                0x0d => "Enter",
                0x10d => "Num Enter",
                0x10 => "Shift",
                0x11 => "Ctrl",
                0x12 => "Alt",
                0x13 => "Pause",
                0x14 => "CapsLock",
                0x1b => "Esc",
                0x20 => "Space",
                0x21 => "Page Up",
                0x22 => "Page Down",
                0x23 => "End",
                0x24 => "Home",
                0x25 => "左方向键",
                0x26 => "上方向键",
                0x27 => "右方向键",
                0x28 => "下方向键",
                0x2c => "PrtSc",
                0x2d => "Insert",
                0x2e => "Delete",
                0x5b => "左 Win",
                0x5c => "右 Win",
                0x5d => "Menu",
                0x6a => "Num *",
                0x6b => "Num +",
                0x6d => "Num -",
                0x6e => "Num .",
                0x6f => "Num /",
                0x90 => "NumLock",
                0x91 => "ScrLk",
                0xa0 => "左 Shift",
                0xa1 => "右 Shift",
                0xa2 => "左 Ctrl",
                0xa3 => "右 Ctrl",
                0xa4 => "左 Alt",
                0xa5 => "右 Alt",
                0xa6 => "浏览器后退",
                0xa7 => "浏览器前进",
                0xa8 => "浏览器刷新",
                0xa9 => "浏览器停止",
                0xaa => "浏览器搜索",
                0xab => "浏览器收藏夹",
                0xac => "浏览器主页",
                0xad => "静音",
                0xae => "音量 -",
                0xaf => "音量 +",
                0xb0 => "下一曲",
                0xb1 => "上一曲",
                0xb2 => "停止播放",
                0xb3 => "播放 / 暂停",
                0xb4 => "邮件",
                0xb5 => "媒体播放器",
                0xb6 => "应用 1",
                0xb7 => "应用 2",
                0xba => ";",
                0xbb => "=",
                0xbc => ",",
                0xbd => "-",
                0xbe => ".",
                0xbf => "/",
                0xc0 => "`",
                0xdb => "[",
                0xdc => "\\",
                0xdd => "]",
                0xde => "'",
                _ => return format!("VK_0x{key:X}"),
            };
            name.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_follow_effective_app_mapping_and_trigger() {
        let mut cfg = HisenseConfig::default();
        cfg.profiles.insert(
            "chrome".into(),
            serde_json::from_value(serde_json::json!({
                "name":"Chrome","process_name":"chrome.exe","enabled":true,
                "button_bindings":{"up":{"type":"SingleKey","value":33}}
            }))
            .unwrap(),
        );
        let effective = cfg.effective("chrome");
        assert_eq!(
            describe(&effective.button_bindings["up"], &effective, Mode::Touch),
            "Page Up"
        );
        assert_eq!(
            describe(&cfg.button_bindings["up"], &cfg, Mode::Touch),
            "上方向键"
        );
        assert_eq!(caption(Some("up"), "click:2"), "上 · 双击");
        assert_eq!(caption(Some("menu"), "long"), "菜单 · 长按");
        assert_eq!(
            describe(&Action::ComboKey(vec![0xa2, 0x43]), &cfg, Mode::Touch),
            "左 Ctrl + C"
        );
        assert_eq!(key_label(0x10d), "Num Enter");
        assert_eq!(key_label(0x87), "F24");
        assert_eq!(
            describe(&Action::PointerConfirm, &cfg, Mode::Move),
            "短按点按／长按移动圆点"
        );
    }
}
