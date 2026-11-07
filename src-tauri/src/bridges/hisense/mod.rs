pub mod config;
pub mod driver;
mod gesture;
mod indicator;
mod input;
mod macro_capture;
pub mod macro_engine;
pub mod mapping;
pub mod pointer;
pub mod profiles;
pub mod protocol;
mod usb;
pub mod visual;
mod voice;

use config::{Action, HisenseConfig};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub running: bool,
    pub connected: bool,
    /// None until a verified receiver report supplies a real remote battery level.
    pub battery_level: Option<u8>,
    pub charging_power_connected: Option<bool>,
    pub packets: u64,
    pub last_button: Option<String>,
    pub last_source: Option<String>,
    pub observed_buttons: BTreeSet<String>,
    pub learning_button: Option<String>,
    pub error: Option<String>,
    pub voice_active: bool,
    pub pointer_mode: String,
    pub microphone: Option<String>,
    pub voice_error: Option<String>,
    pub voice_level: f32,
    pub audio_frames: u64,
    pub active_profile: String,
    pub active_profile_name: String,
    pub active_application: String,
    pub active_macros: usize,
}
struct Learn {
    button: String,
    deadline: Instant,
}
pub struct Runtime {
    config: RwLock<HisenseConfig>,
    path: PathBuf,
    status: Mutex<Status>,
    learning: Mutex<Option<Learn>>,
    stop: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
    lifecycle: Mutex<()>,
    mapping: Mutex<mapping::Mapping<Action>>,
    revision: AtomicU64,
    macro_engine: Mutex<macro_engine::Engine>,
    macro_stop: AtomicBool,
}
impl Runtime {
    pub fn auto_connect(&self) -> bool {
        self.config.read().auto_connect
    }
    pub fn new(path: PathBuf) -> Self {
        let (config, error) = match HisenseConfig::load(&path) {
            Ok(c) => (c, None),
            Err(e) => (
                HisenseConfig::default(),
                Some(format!("海信配置读取失败，已暂停自动连接：{e}")),
            ),
        };
        let mut config = config;
        if error.is_some() {
            config.auto_connect = false;
        }
        Self {
            config: RwLock::new(config),
            path,
            status: Mutex::new(Status {
                error,
                pointer_mode: "touch".into(),
                active_profile: "default".into(),
                active_profile_name: "桌面默认".into(),
                ..Default::default()
            }),
            learning: Mutex::new(None),
            stop: AtomicBool::new(true),
            worker: Mutex::new(None),
            lifecycle: Mutex::new(()),
            mapping: Mutex::new(mapping::Mapping::default()),
            revision: AtomicU64::new(0),
            macro_engine: Mutex::new(macro_engine::Engine::default()),
            macro_stop: AtomicBool::new(false),
        }
    }
    pub fn start(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        let _guard = self.lifecycle.lock();
        if self
            .worker
            .lock()
            .as_ref()
            .is_some_and(|h| !h.is_finished())
        {
            return Ok(());
        }
        if let Some(old) = self.worker.lock().take() {
            let _ = old.join();
        }
        self.stop.store(false, Ordering::Release);
        self.status.lock().running = true;
        let runtime = self.clone();
        let handle = std::thread::Builder::new()
            .name("hisense-usb".into())
            .spawn(move || runtime.run(app))
            .map_err(|e| {
                self.status.lock().running = false;
                self.stop.store(true, Ordering::Release);
                e.to_string()
            })?;
        *self.worker.lock() = Some(handle);
        Ok(())
    }
    pub fn stop(&self) {
        let _guard = self.lifecycle.lock();
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.lock().take() {
            let _ = worker.join();
        }
        let mut s = self.status.lock();
        s.running = false;
        s.connected = false;
        s.battery_level = None;
        s.charging_power_connected = None;
        s.voice_active = false;
        s.active_macros = 0;
        s.pointer_mode = "touch".into();
        s.learning_button = None;
        self.learning.lock().take();
    }
    fn save(&self, mut cfg: HisenseConfig) -> Result<(), String> {
        let mut current = self.config.write();
        cfg.button_sources = current.button_sources.clone();
        cfg.validate()?;
        cfg.save(&self.path)?;
        *current = cfg;
        self.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
    // Visual toggles never cancel a held touch or keyboard input.
    fn toggle_highlighter(&self) -> Result<(bool, bool), String> {
        let mut current = self.config.write();
        let mut next = current.clone();
        next.highlight_trail_enabled = !next.highlight_trail_enabled;
        if next.highlight_trail_enabled {
            next.touch_indicator_enabled = true;
        }
        next.save(&self.path)?;
        let flags = (next.highlight_trail_enabled, next.touch_indicator_enabled);
        *current = next;
        Ok(flags)
    }
    fn apply_mappings(&self, document: config::MappingsFile) -> Result<HisenseConfig, String> {
        let mut current = self.config.write();
        let mut next = current.clone();
        document.apply(&mut next)?;
        next.save(&self.path)?;
        *current = next.clone();
        self.revision.fetch_add(1, Ordering::Release);
        Ok(next)
    }
    fn learn(&self, source: &str) -> bool {
        let Some(learn) = self.learning.lock().take() else {
            return false;
        };
        if !protocol::learnable_source(source) {
            *self.learning.lock() = Some(learn);
            return false;
        }
        let mut current = self.config.write();
        let mut cfg = current.clone();
        cfg.button_sources
            .insert(source.into(), learn.button.clone());
        match cfg.save(&self.path) {
            Ok(()) => {
                *current = cfg;
                self.revision.fetch_add(1, Ordering::Release);
                let mut s = self.status.lock();
                s.learning_button = None;
                s.observed_buttons.insert(learn.button);
                s.error = None;
            }
            Err(e) => {
                let mut s = self.status.lock();
                s.learning_button = None;
                s.error = Some(e);
            }
        }
        true
    }
    fn run(self: Arc<Self>, app: AppHandle) {
        let voice = voice::Voice::new();
        let mut output = input::Output::default();
        let mut held: BTreeMap<String, Held> = BTreeMap::new();
        let mut mouse = MouseOwners::default();
        let indicator = indicator::Indicator::new();
        let mut last_revision = self.revision.load(Ordering::Acquire);
        let mut cached_cfg = self.config.read().clone();
        let mut refresh_config = Instant::now();
        let mut active_profile = String::new();
        let mut last_process = String::new();
        while !self.stop.load(Ordering::Acquire) {
            let usb = match usb::Usb::open() {
                Ok(d) => d,
                Err(e) => {
                    let mut s = self.status.lock();
                    s.connected = false;
                    s.battery_level = None;
                    s.charging_power_connected = None;
                    s.error = Some(e);
                    drop(s);
                    for _ in 0..20 {
                        if self.stop.load(Ordering::Acquire) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    continue;
                }
            };
            {
                let mut s = self.status.lock();
                s.connected = true;
                s.battery_level = None;
                s.charging_power_connected = None;
                s.error = None;
            }
            log::info!("HISENSE receiver connected transport=winusb input_interface=02");
            let mut previous = BTreeSet::new();
            let mut pen_sources = BTreeSet::new();
            let mut raw: Option<(String, Instant)> = None;
            let mut filter = pointer::PointerFilter::default();
            let mut last_pen = Instant::now();
            let mut pointer_visible = false;
            let mut bytes = [0u8; 512];
            while !self.stop.load(Ordering::Acquire) {
                let n = match usb.read(&mut bytes) {
                    Ok(n) => n,
                    Err(e) => {
                        self.status.lock().error = Some(e);
                        break;
                    }
                };
                let now = Instant::now();
                let revision = self.revision.load(Ordering::Acquire);
                if now >= refresh_config || revision != last_revision {
                    let base = self.config.read().clone();
                    let process = crate::logging::foreground_process_name().unwrap_or_default();
                    let profile = base.profile_for(&process);
                    if profile != active_profile || process != last_process {
                        self.mapping.lock().clear();
                        apply_macro_events(
                            self.macro_engine.lock().clear(),
                            &mut output,
                            &mut mouse,
                        );
                        release_all(&mut held, &mut output, &mut mouse, &voice);
                        previous.clear();
                        pen_sources.clear();
                        filter.reset();
                        if profile != active_profile {
                            let name = base
                                .profiles
                                .get(&profile)
                                .map(|p| p.name.clone())
                                .unwrap_or_else(|| "桌面默认".into());
                            indicator.notice(indicator::Notice::Profile(format!("已切换：{name}")));
                            let mut status = self.status.lock();
                            status.active_profile = profile.clone();
                            status.active_profile_name = name;
                        }
                        active_profile = profile;
                        last_process = process.clone();
                    }
                    self.status.lock().active_application = process;
                    cached_cfg = base.effective(&active_profile);
                    refresh_config = now + Duration::from_millis(100);
                }
                let cfg = &cached_cfg;
                if revision != last_revision {
                    self.mapping.lock().clear();
                    apply_macro_events(self.macro_engine.lock().clear(), &mut output, &mut mouse);
                    release_all(&mut held, &mut output, &mut mouse, &voice);
                    last_revision = revision;
                }
                output.gesture_threshold(cfg.pointer_hold_ms);
                indicator.set_style(cfg.indicator_style());
                let capturing = macro_capture::active() || app
                    .try_state::<crate::bridges::shared::shortcut_capture::ShortcutCaptureSession>()
                    .is_some_and(|s| s.is_active());
                if capturing {
                    pointer_visible = false;
                    self.mapping.lock().clear();
                    apply_macro_events(self.macro_engine.lock().clear(), &mut output, &mut mouse);
                    release_all(&mut held, &mut output, &mut mouse, &voice);
                    filter.reset();
                    previous.clear();
                    pen_sources.clear();
                }
                if n > 0 {
                    self.status.lock().packets += 1;
                    if let Some(powered) = protocol::charging_power(&bytes[..n]) {
                        self.status.lock().charging_power_connected = Some(powered);
                        log::info!("HISENSE charging power connected={powered}");
                    } else if let Some(pen) = protocol::pen(&bytes[..n]) {
                        let pen_dt = now.duration_since(last_pen).as_secs_f64();
                        last_pen = now;
                        let was_visible = pointer_visible;
                        pointer_visible = pen.valid && cfg.pointer_enabled && !capturing;
                        if pen.valid && !capturing {
                            let sources = [("pen:tip", pen.tip), ("pen:barrel", pen.barrel)]
                                .into_iter()
                                .filter(|(_, down)| *down)
                                .map(|(s, _)| s.to_string())
                                .collect();
                            self.edges(
                                &app,
                                &cfg,
                                &mut pen_sources,
                                sources,
                                &mut held,
                                &mut output,
                                &mut mouse,
                                &voice,
                            );
                            if cfg.pointer_enabled && output.pointer_motion_active() {
                                let position = filter.update(
                                    (pen.x, pen.y),
                                    input::cursor(),
                                    input::bounds(),
                                    cfg.sensitivity,
                                    cfg.deadzone_pixels,
                                    cfg.smoothing_ms,
                                    pen_dt,
                                    cfg.adaptive_sensitivity.then_some(cfg.precision_ratio),
                                );
                                input::move_to(position);
                            } else {
                                filter.reset();
                            }
                        } else {
                            if was_visible {
                                output.cancel_gestures();
                                output.touch_cancel();
                            }
                            filter.reset();
                            self.edges(
                                &app,
                                &cfg,
                                &mut pen_sources,
                                BTreeSet::new(),
                                &mut held,
                                &mut output,
                                &mut mouse,
                                &voice,
                            );
                        }
                    } else if !capturing {
                        if let Some(keys) = protocol::keyboard(&bytes[..n]) {
                            self.edges(
                                &app,
                                &cfg,
                                &mut previous,
                                keys,
                                &mut held,
                                &mut output,
                                &mut mouse,
                                &voice,
                            );
                        } else if let Some(source) = protocol::raw_source(&bytes[..n]) {
                            if raw.as_ref().is_some_and(|r| r.0 != source) {
                                if let Some((old, _)) = raw.take() {
                                    self.mapping_release(
                                        &app,
                                        &old,
                                        &mut held,
                                        &mut output,
                                        &mut mouse,
                                        &voice,
                                    );
                                }
                            }
                            if raw.is_none() {
                                self.press(
                                    &app,
                                    &cfg,
                                    &source,
                                    &mut held,
                                    &mut output,
                                    &mut mouse,
                                    &voice,
                                );
                            }
                            raw = Some((source, now));
                        }
                    }
                }
                if last_pen.elapsed() > Duration::from_millis(200) {
                    if pointer_visible {
                        output.cancel_gestures();
                        output.touch_cancel();
                    }
                    pointer_visible = false;
                    filter.reset();
                    self.edges(
                        &app,
                        &cfg,
                        &mut pen_sources,
                        BTreeSet::new(),
                        &mut held,
                        &mut output,
                        &mut mouse,
                        &voice,
                    );
                }
                if raw
                    .as_ref()
                    .is_some_and(|(_, time)| time.elapsed() > Duration::from_millis(120))
                {
                    if let Some((s, _)) = raw.take() {
                        self.mapping_release(&app, &s, &mut held, &mut output, &mut mouse, &voice);
                    }
                }
                let events = self.mapping.lock().tick(now);
                self.mapping_events(
                    &app,
                    events,
                    &cfg,
                    &mut held,
                    &mut output,
                    &mut mouse,
                    &voice,
                );
                for h in held.values_mut() {
                    if now >= h.repeat_at {
                        if let Action::MouseMove(motion) = &h.action {
                            move_mouse(motion, h.started, now);
                            h.repeat_at = now + Duration::from_millis(16);
                            continue;
                        }
                        if let Some(&key) = h.keys.last() {
                            if !matches!(key,0x10..=0x12|0x5b|0x5c|0xa0..=0xa5) {
                                input::key(key, true);
                            }
                        }
                        h.repeat_at = now + Duration::from_millis(65);
                    }
                }
                if self
                    .learning
                    .lock()
                    .as_ref()
                    .is_some_and(|l| l.deadline < now)
                {
                    self.learning.lock().take();
                    let mut s = self.status.lock();
                    s.learning_button = None;
                    s.error = Some(
                        "未收到该键的数据；未修改映射。若按键只发红外，需接收硬件支持后才能映射。"
                            .into(),
                    );
                }
                if self.macro_stop.swap(false, Ordering::AcqRel) {
                    apply_macro_events(self.macro_engine.lock().clear(), &mut output, &mut mouse);
                }
                apply_macro_events(self.macro_engine.lock().tick(now), &mut output, &mut mouse);
                self.status.lock().active_macros = self.macro_engine.lock().active();
                if let Err(error) = output.gesture_tick().and_then(|_| output.touch_update()) {
                    self.status.lock().error = Some(error);
                    output.cancel_gestures();
                    output.touch_cancel();
                }
                let pressed = output.touch_active();
                if let Some(enabled) = output.take_mode_notice() {
                    indicator.notice(enabled);
                }
                indicator.update(
                    cfg.touch_indicator_enabled && !capturing && (pointer_visible || pressed),
                    input::touch_cursor().unwrap_or_else(|_| input::cursor()),
                    pressed,
                );
                if let Some(error) = indicator.take_error() {
                    self.status.lock().error = Some(error);
                }
                let vs = voice.status.lock();
                let mut s = self.status.lock();
                s.voice_active = vs.active;
                s.pointer_mode = output.pointer_mode().as_str().into();
                s.microphone = vs.microphone.clone();
                s.voice_error = vs.error.clone();
                s.voice_level = f32::from_bits(voice.peak.load(Ordering::Relaxed));
                s.audio_frames = voice.frames.load(Ordering::Relaxed);
            }
            self.mapping.lock().clear();
            apply_macro_events(self.macro_engine.lock().clear(), &mut output, &mut mouse);
            release_all(&mut held, &mut output, &mut mouse, &voice);
            indicator.update(false, (0, 0), false);
            self.learning.lock().take();
            let mut status = self.status.lock();
            status.connected = false;
            status.active_macros = 0;
            status.battery_level = None;
            status.charging_power_connected = None;
            status.voice_active = false;
            status.voice_level = 0.;
            status.learning_button = None;
        }
        self.mapping.lock().clear();
        release_all(&mut held, &mut output, &mut mouse, &voice);
        self.status.lock().running = false;
    }
    #[allow(clippy::too_many_arguments)]
    fn edges(
        &self,
        app: &AppHandle,
        cfg: &HisenseConfig,
        old: &mut BTreeSet<String>,
        new: BTreeSet<String>,
        held: &mut BTreeMap<String, Held>,
        out: &mut input::Output,
        mouse: &mut MouseOwners,
        voice: &voice::Voice,
    ) {
        for s in old.difference(&new) {
            self.mapping_release(app, s, held, out, mouse, voice);
        }
        for s in new.difference(old) {
            self.press(app, cfg, s, held, out, mouse, voice);
        }
        *old = new;
    }
    #[allow(clippy::too_many_arguments)]
    fn press(
        &self,
        app: &AppHandle,
        cfg: &HisenseConfig,
        source: &str,
        held: &mut BTreeMap<String, Held>,
        out: &mut input::Output,
        mouse: &mut MouseOwners,
        voice: &voice::Voice,
    ) {
        if self.learn(source) {
            return;
        }
        let button = cfg.button_sources.get(source).cloned();
        {
            let mut s = self.status.lock();
            s.last_source = Some(source.into());
            s.last_button = button.clone();
            if let Some(b) = &button {
                s.observed_buttons.insert(b.clone());
            }
        }
        let _ = app.emit(
            "hisense-key-event",
            serde_json::json!({"source":source,"button":button,"pressed":true}),
        );
        let action = if let Some(button) = &button {
            cfg.button_bindings
                .get(button)
                .cloned()
                .unwrap_or(Action::None)
        } else {
            protocol::usage_vk(source)
                .map(Action::SingleKey)
                .unwrap_or(Action::None)
        };
        if let Some(button) = button.filter(|_| !cfg.realtime(&action)) {
            let long = cfg
                .long_press_bindings
                .get(&button)
                .filter(|a| !matches!(a, Action::None))
                .cloned();
            let multi: BTreeMap<_, _> = cfg
                .multi_click_bindings
                .get(&button)
                .into_iter()
                .flat_map(|slots| slots.iter())
                .filter(|(_, a)| !matches!(a, Action::None))
                .map(|(n, a)| (*n, a.clone()))
                .collect();
            if long.is_some() || !multi.is_empty() {
                let plan = mapping::Plan {
                    single: action,
                    long,
                    multi,
                    interval: Duration::from_millis(cfg.multi_click_interval_ms),
                    hold: Duration::from_millis(
                        if matches!(button.as_str(), "volume_up" | "volume_down") {
                            400
                        } else {
                            280
                        },
                    ),
                };
                let events = self
                    .mapping
                    .lock()
                    .down(&button, source, plan, Instant::now());
                self.mapping_events(app, events, cfg, held, out, mouse, voice);
                return;
            }
        }
        self.execute_action(app, source, action, cfg, held, out, mouse, voice, true);
    }
    fn mapping_release(
        &self,
        app: &AppHandle,
        source: &str,
        held: &mut BTreeMap<String, Held>,
        out: &mut input::Output,
        mouse: &mut MouseOwners,
        voice: &voice::Voice,
    ) {
        let events = self.mapping.lock().up(source, Instant::now());
        if let Some(events) = events {
            let cfg = self.config.read().clone();
            self.mapping_events(app, events, &cfg, held, out, mouse, voice);
        } else {
            apply_macro_events(
                self.macro_engine.lock().up(source, Instant::now()),
                out,
                mouse,
            );
            release(source, held, out, mouse, voice);
        }
    }
    fn mapping_events(
        &self,
        app: &AppHandle,
        events: Vec<mapping::Event<Action>>,
        cfg: &HisenseConfig,
        held: &mut BTreeMap<String, Held>,
        out: &mut input::Output,
        mouse: &mut MouseOwners,
        voice: &voice::Voice,
    ) {
        for event in events {
            match event {
                mapping::Event::Up(owner) => {
                    apply_macro_events(
                        self.macro_engine.lock().up(&owner, Instant::now()),
                        out,
                        mouse,
                    );
                    release(&owner, held, out, mouse, voice);
                }
                mapping::Event::Down {
                    owner,
                    button,
                    trigger,
                    action,
                } => {
                    log::info!("HISENSE MAPPING button={button} trigger={trigger}");
                    self.execute_action(
                        app,
                        &owner,
                        action,
                        cfg,
                        held,
                        out,
                        mouse,
                        voice,
                        trigger == "long",
                    );
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn execute_action(
        &self,
        app: &AppHandle,
        source: &str,
        action: Action,
        cfg: &HisenseConfig,
        held: &mut BTreeMap<String, Held>,
        out: &mut input::Output,
        mouse: &mut MouseOwners,
        voice: &voice::Voice,
        repeat: bool,
    ) {
        let keys = match &action {
            Action::SingleKey(k) => vec![*k],
            Action::ComboKey(k) => k.clone(),
            _ => Vec::new(),
        };
        if !keys.is_empty() && !out.press(&keys) {
            self.status.lock().error = Some("按键发送失败，请检查目标程序权限".into());
            return;
        }
        match &action {
            Action::MouseMove(motion) => move_mouse(motion, Instant::now(), Instant::now()),
            Action::FocusChatGpt => {
                std::thread::spawn(crate::chatgpt_focus::activate_chatgpt_and_focus_input);
            }
            Action::MouseClick => mouse.set(source, false, true),
            Action::MouseRightClick => mouse.set(source, true, true),
            Action::TouchClick => {
                if let Err(error) = out.touch_press(source) {
                    self.status.lock().error = Some(error);
                    return;
                }
            }
            Action::PointerConfirm => {
                if let Err(error) = out.confirm_down(source) {
                    self.status.lock().error = Some(error);
                    out.touch_cancel();
                }
            }
            Action::PointerAi => {
                if let Err(error) = out.ai_down(source) {
                    self.status.lock().error = Some(error);
                    out.touch_cancel();
                }
            }
            Action::Macro(id) => {
                if let Some(definition) = cfg.macros.get(id) {
                    match self
                        .macro_engine
                        .lock()
                        .down(source, id, definition, Instant::now())
                    {
                        Ok(events) => apply_macro_events(events, out, mouse),
                        Err(e) => self.status.lock().error = Some(e),
                    }
                }
            }
            Action::StopMacros => {
                self.macro_stop.store(true, Ordering::Release);
            }
            Action::CycleProfile | Action::SelectProfile(_) => {
                let mut current = self.config.write();
                if !current.profile_switching_enabled {
                    out.profile_notice("配置文件切换已关闭".into());
                    return;
                }
                let mut next = current.clone();
                if let Action::SelectProfile(id) = &action {
                    if !id.is_empty()
                        && id != "default"
                        && !next.profiles.get(id).is_some_and(|p| p.enabled)
                    {
                        self.status.lock().error =
                            Some("目标配置已停用，请先在应用配置管理中启用".into());
                        return;
                    }
                    next.fixed_profile = if id.is_empty() {
                        None
                    } else {
                        Some(id.clone())
                    };
                } else {
                    let mut ids = vec![None, Some("default".into())];
                    ids.extend(
                        next.profiles
                            .iter()
                            .filter(|(_, p)| p.enabled)
                            .map(|(id, _)| Some(id.clone())),
                    );
                    let index = ids
                        .iter()
                        .position(|id| id == &next.fixed_profile)
                        .unwrap_or(0);
                    next.fixed_profile = ids[(index + 1) % ids.len()].clone();
                }
                match next.save(&self.path) {
                    Ok(()) => {
                        *current = next.clone();
                        let name = next
                            .fixed_profile
                            .as_ref()
                            .map(|id| {
                                if id == "default" {
                                    "桌面默认".into()
                                } else {
                                    next.profiles
                                        .get(id)
                                        .map(|p| p.name.clone())
                                        .unwrap_or_else(|| "桌面默认".into())
                                }
                            })
                            .unwrap_or_else(|| "按应用自动切换".into());
                        out.profile_notice(format!("配置：{name}"));
                        self.revision.fetch_add(1, Ordering::Release);
                        let _=app.emit("hisense-profile-settings",serde_json::json!({"fixed_profile":next.fixed_profile,"profile_switching_enabled":next.profile_switching_enabled}));
                    }
                    Err(e) => self.status.lock().error = Some(e),
                }
            }
            Action::ToggleHighlighter => match self.toggle_highlighter() {
                Ok((enabled, indicator)) => {
                    out.trail_notice(enabled);
                    let _ = app.emit(
                        "hisense-visual-settings",
                        serde_json::json!({
                            "highlight_trail_enabled": enabled, "touch_indicator_enabled": indicator
                        }),
                    );
                }
                Err(error) => self.status.lock().error = Some(format!("荧光笔切换失败：{error}")),
            },
            Action::TouchDrag => {
                if let Err(error) = out.drag_down(source) {
                    self.status.lock().error = Some(error);
                    return;
                }
            }
            Action::VoiceInput => voice.down(source, cfg.clone()),
            Action::TextInput(text) => input::text(text),
            Action::LaunchApp(path) => {
                if let Err(e) = crate::windows_command::background_command(path).spawn() {
                    self.status.lock().error = Some(format!("应用启动失败：{e}"));
                }
            }
            _ => {}
        }
        let repeat_delay = if !repeat {
            86_400_000
        } else if matches!(action, Action::MouseMove(_)) {
            16
        } else {
            450
        };
        held.insert(
            source.into(),
            Held {
                action,
                keys,
                started: Instant::now(),
                repeat_at: Instant::now() + Duration::from_millis(repeat_delay),
            },
        );
    }
}
fn apply_macro_events(
    events: Vec<macro_engine::Event>,
    out: &mut input::Output,
    mouse: &mut MouseOwners,
) {
    for event in events {
        match event {
            macro_engine::Event::Key { key, down: true } => {
                out.press(&[key]);
            }
            macro_engine::Event::Key { key, down: false } => out.release(&[key]),
            macro_engine::Event::Mouse { owner, right, down } => mouse.set(&owner, right, down),
        }
    }
}
fn move_mouse(motion: &config::MouseMove, started: Instant, now: Instant) {
    let frames = now.saturating_duration_since(started).as_millis() / 16;
    let step = if motion.accelerate {
        (motion.step as u128 + frames / 10).min(motion.step as u128 * 4) as i32
    } else {
        motion.step as i32
    };
    let p = input::cursor();
    let bounds = input::bounds();
    let x = (p.0 + motion.dx * step)
        .clamp(bounds.left as i32, (bounds.left + bounds.width - 1.) as i32);
    let y =
        (p.1 + motion.dy * step).clamp(bounds.top as i32, (bounds.top + bounds.height - 1.) as i32);
    input::move_to((x, y));
}
struct Held {
    started: Instant,
    action: Action,
    keys: Vec<u16>,
    repeat_at: Instant,
}
#[derive(Default)]
struct MouseOwners {
    left: BTreeSet<String>,
    right: BTreeSet<String>,
}
impl MouseOwners {
    fn set(&mut self, source: &str, right: bool, down: bool) {
        let owners = if right {
            &mut self.right
        } else {
            &mut self.left
        };
        let before = !owners.is_empty();
        if down {
            owners.insert(source.into());
        } else {
            owners.remove(source);
        }
        let after = !owners.is_empty();
        if before != after {
            input::mouse(right, after);
        }
    }
    fn clear(&mut self) {
        if !self.left.is_empty() {
            input::mouse(false, false);
        }
        if !self.right.is_empty() {
            input::mouse(true, false);
        }
        self.left.clear();
        self.right.clear();
    }
}
fn release(
    source: &str,
    held: &mut BTreeMap<String, Held>,
    out: &mut input::Output,
    mouse: &mut MouseOwners,
    voice: &voice::Voice,
) {
    if let Some(h) = held.remove(source) {
        out.release(&h.keys);
        match h.action {
            Action::MouseClick => mouse.set(source, false, false),
            Action::MouseRightClick => mouse.set(source, true, false),
            Action::TouchClick => {
                if let Err(error) = out.touch_release(source) {
                    log::warn!("HISENSE touch release failed: {error}");
                    out.touch_cancel();
                }
            }
            Action::PointerConfirm => {
                if let Err(error) = out.confirm_up(source) {
                    log::warn!("HISENSE confirm tap failed: {error}");
                    out.touch_cancel();
                }
            }
            Action::PointerAi => {
                if let Err(error) = out.ai_up(source) {
                    log::warn!("HISENSE AI release failed: {error}");
                    out.touch_cancel();
                }
            }
            Action::TouchDrag => {
                if let Err(error) = out.drag_up(source) {
                    log::warn!("HISENSE drag release failed: {error}");
                    out.touch_cancel();
                }
            }
            Action::VoiceInput => voice.up(source),
            _ => {}
        }
    }
}
fn release_all(
    held: &mut BTreeMap<String, Held>,
    out: &mut input::Output,
    mouse: &mut MouseOwners,
    voice: &voice::Voice,
) {
    out.cancel_gestures();
    for source in held.keys().cloned().collect::<Vec<_>>() {
        release(&source, held, out, mouse, voice);
    }
    out.release_all();
    mouse.clear();
    voice.cancel();
}

#[tauri::command]
pub fn hisense_status(runtime: State<'_, Arc<Runtime>>) -> Status {
    runtime.status.lock().clone()
}
#[tauri::command]
pub fn hisense_config(runtime: State<'_, Arc<Runtime>>) -> HisenseConfig {
    runtime.config.read().clone()
}
#[tauri::command]
pub fn hisense_save_config(
    config: HisenseConfig,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<(), String> {
    runtime.save(config)
}
#[tauri::command]
pub async fn hisense_export_mappings(
    path: String,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<(), String> {
    let document = config::MappingsFile::from_config(&runtime.config.read());
    let text = serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("导出失败：{e}"))
}
#[tauri::command]
pub async fn hisense_import_mappings(
    path: String,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<HisenseConfig, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("读取失败：{e}"))?;
    use std::io::Read;
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(1_048_577)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 1_048_576 {
        return Err("映射文件不能超过 1 MB".into());
    }
    let document = serde_json::from_str::<config::MappingsFile>(&text)
        .map_err(|e| format!("映射格式错误：{e}"))?;
    runtime.apply_mappings(document)
}
#[tauri::command]
pub fn hisense_reset_mappings(
    profile: Option<String>,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<HisenseConfig, String> {
    let mut next = runtime.config.read().clone();
    if let Some(id) = profile.filter(|id| id != "default") {
        let p = next.profiles.get_mut(&id).ok_or("应用配置不存在")?;
        p.button_bindings.clear();
        p.long_press_bindings.clear();
        p.multi_click_bindings.clear();
    } else {
        next.button_bindings = HisenseConfig::default().button_bindings;
        next.long_press_bindings.clear();
        next.multi_click_bindings.clear();
    }
    runtime.save(next)?;
    let saved = runtime.config.read().clone();
    Ok(saved)
}
#[tauri::command]
pub fn hisense_start(app: AppHandle, runtime: State<'_, Arc<Runtime>>) -> Result<(), String> {
    runtime.inner().start(app)
}
#[tauri::command]
pub async fn hisense_stop(runtime: State<'_, Arc<Runtime>>) -> Result<(), String> {
    let r = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || r.stop())
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn hisense_learn(
    button: Option<String>,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<(), String> {
    if let Some(b) = &button {
        if !runtime.status.lock().connected {
            return Err("请先连接海信接收器再学习按键".into());
        }
        if !config::BUTTONS.iter().any(|(id, _)| id == b) {
            return Err("未知按键".into());
        }
    }
    *runtime.learning.lock() = button.clone().map(|button| Learn {
        button,
        deadline: Instant::now() + Duration::from_secs(12),
    });
    runtime.status.lock().learning_button = button;
    runtime.revision.fetch_add(1, Ordering::Release);
    Ok(())
}
#[tauri::command]
pub async fn hisense_setup_receiver(
    enabled: bool,
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<String, String> {
    let r = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = SETUP_LOCK.lock();
        r.stop();
        let result = driver::request(enabled);
        if enabled {
            let _ = r.start(app);
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
static SETUP_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_import_save_keeps_current_mapping_and_revision() {
        let directory = std::env::temp_dir().join(format!(
            "nexus-hisense-failed-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let runtime = Runtime::new(directory.join("blocked.json"));
        std::fs::create_dir(&runtime.path).unwrap();
        let before = serde_json::to_value(&*runtime.config.read()).unwrap();
        let mut changed = HisenseConfig::default();
        changed
            .button_bindings
            .insert("home".into(), Action::SingleKey(65));
        assert!(runtime
            .apply_mappings(config::MappingsFile::from_config(&changed))
            .is_err());
        assert_eq!(
            serde_json::to_value(&*runtime.config.read()).unwrap(),
            before
        );
        assert_eq!(runtime.revision.load(Ordering::Acquire), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn saving_stale_frontend_settings_preserves_newly_learned_source() {
        let directory = std::env::temp_dir().join(format!(
            "nexus-hisense-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let runtime = Runtime::new(directory.join("hisense.json"));
        let mut edited = runtime.config.read().clone();
        *runtime.learning.lock() = Some(Learn {
            button: "mic".into(),
            deadline: Instant::now() + Duration::from_secs(12),
        });
        assert!(runtime.learn("key:3e"));
        edited.sensitivity = 3.;
        runtime.save(edited).unwrap();
        let saved = HisenseConfig::load(&directory.join("hisense.json")).unwrap();
        assert_eq!(
            saved.button_sources.get("key:3e").map(String::as_str),
            Some("mic")
        );
        assert_eq!(saved.sensitivity, 3.);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[tauri::command]
pub fn hisense_macro_record_start(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<u64, String> {
    if app
        .try_state::<crate::bridges::shared::shortcut_capture::ShortcutCaptureSession>()
        .is_some_and(|s| s.is_active())
    {
        return Err("请先结束快捷键录入".into());
    }
    runtime.macro_stop.store(true, Ordering::Release);
    for _ in 0..20 {
        if runtime.macro_engine.lock().active() == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Baseline check only, before capture starts; recorded events use hook edges.
    if (8..=0xfe).any(|vk| unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) } as u16 & 0x8000 != 0) {
        return Err("请先松开电脑键盘上的所有按键，再开始录制".into());
    }
    crate::bridges::xiaomi::special_keys::ensure_hook_for_capture();
    for _ in 0..20 {
        if crate::bridges::xiaomi::special_keys::is_hook_running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !crate::bridges::xiaomi::special_keys::is_hook_running() {
        return Err("键盘录制尚未就绪，请稍后重试".into());
    }
    let _ = crate::bridges::xiaomi::special_keys::bump_hook_to_front_and_settle(100);
    runtime.macro_stop.store(true, Ordering::Release);
    macro_capture::start()
}
#[tauri::command]
pub fn hisense_macro_record_poll(
    session: u64,
    stop: bool,
) -> Result<macro_capture::Snapshot, String> {
    macro_capture::snapshot(session, stop)
}
#[tauri::command]
pub fn hisense_stop_macros(runtime: State<'_, Arc<Runtime>>) {
    runtime.macro_stop.store(true, Ordering::Release);
}

pub(crate) fn record_macro_key(vk: u32, msg: u32, injected: bool, extended: bool) -> bool {
    macro_capture::key(vk, msg, injected, extended)
}
