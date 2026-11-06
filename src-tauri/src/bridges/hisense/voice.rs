use super::config::HisenseConfig;
use crate::bridges::xiaomi::{key_mapping::IndependentVoiceShortcut, voice_pcm};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::{
    collections::{BTreeSet, VecDeque},
    net::UdpSocket,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::JoinHandle,
};

#[derive(Default)]
struct Requests {
    owners: BTreeSet<String>,
    generation: u64,
}
enum Command {
    Start(u64, HisenseConfig),
    Stop(u64),
    Quit,
}
#[derive(Default)]
pub struct VoiceStatus {
    pub active: bool,
    pub microphone: Option<String>,
    pub error: Option<String>,
}
pub struct Voice {
    requests: Arc<Mutex<Requests>>,
    tx: SyncSender<Command>,
    worker: Option<JoinHandle<()>>,
    pub status: Arc<Mutex<VoiceStatus>>,
    pub peak: Arc<AtomicU32>,
    pub frames: Arc<AtomicU64>,
}
impl Voice {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(64);
        let requests = Arc::new(Mutex::new(Requests::default()));
        let status = Arc::new(Mutex::new(VoiceStatus::default()));
        let peak = Arc::new(AtomicU32::new(0));
        let frames = Arc::new(AtomicU64::new(0));
        let (r, s, p, f) = (
            requests.clone(),
            status.clone(),
            peak.clone(),
            frames.clone(),
        );
        let worker = std::thread::Builder::new()
            .name("hisense-voice".into())
            .spawn(move || {
                let mut active: Option<(u64, Capture, IndependentVoiceShortcut)> = None;
                let finish = |active: &mut Option<(u64, Capture, IndependentVoiceShortcut)>| {
                    if let Some((_, capture, shortcut)) = active.take() {
                        drop(capture);
                        drop(shortcut);
                        crate::audio::microphone_router::voice_released();
                        voice_pcm::release_external_voice();
                        s.lock().active = false;
                        p.store(0, Ordering::Relaxed);
                    }
                };
                loop {
                    // Release even if a saturated command queue dropped the Stop message.
                    if active.as_ref().is_some_and(|a| !current(&r, a.0)) {
                        finish(&mut active);
                    }
                    let command = match rx.recv_timeout(std::time::Duration::from_millis(20)) {
                        Ok(c) => c,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    match command {
                        Command::Quit => break,
                        Command::Stop(id) => {
                            if active.as_ref().is_some_and(|a| a.0 == id) {
                                finish(&mut active);
                            }
                        }
                        Command::Start(id, cfg) => {
                            finish(&mut active);
                            if !current(&r, id) {
                                continue;
                            }
                            if !voice_pcm::claim_external_voice() {
                                s.lock().error =
                                    Some("其他遥控器正在使用语音，请先松开它的语音键".into());
                                continue;
                            }
                            let capture = match Capture::start(&cfg, p.clone(), f.clone()) {
                                Ok(c) => c,
                                Err(e) => {
                                    voice_pcm::release_external_voice();
                                    s.lock().error = Some(e);
                                    continue;
                                }
                            };
                            s.lock().microphone = Some(capture.name.clone());
                            crate::audio::microphone_router::voice_pressed();
                            if !current(&r, id) {
                                drop(capture);
                                crate::audio::microphone_router::voice_released();
                                voice_pcm::release_external_voice();
                                continue;
                            }
                            let shortcut = match IndependentVoiceShortcut::press(
                                &cfg.voice_keys,
                                &cfg.voice_profile,
                                cfg.voice_mode == "Hold",
                            ) {
                                Ok(shortcut) => shortcut,
                                Err(e) => {
                                    drop(capture);
                                    crate::audio::microphone_router::voice_released();
                                    voice_pcm::release_external_voice();
                                    s.lock().error = Some(e);
                                    continue;
                                }
                            };
                            if !current(&r, id) {
                                drop(capture);
                                drop(shortcut);
                                crate::audio::microphone_router::voice_released();
                                voice_pcm::release_external_voice();
                                continue;
                            }
                            capture.open_gate();
                            s.lock().active = true;
                            s.lock().error = None;
                            log::info!(
                                "HISENSE VOICE session={id} mode={} ime_capture=unverified",
                                cfg.voice_mode
                            );
                            active = Some((id, capture, shortcut));
                        }
                    }
                }
                finish(&mut active);
            })
            .expect("hisense voice worker");
        Self {
            requests,
            tx,
            worker: Some(worker),
            status,
            peak,
            frames,
        }
    }
    pub fn down(&self, source: &str, cfg: HisenseConfig) {
        let mut r = self.requests.lock();
        let was_empty = r.owners.is_empty();
        r.owners.insert(source.into());
        if was_empty {
            r.generation = r.generation.wrapping_add(1);
            let id = r.generation;
            if self.tx.try_send(Command::Start(id, cfg)).is_err() {
                r.owners.clear();
                self.status.lock().error = Some("语音任务繁忙，请松开后重试".into());
            }
        }
    }
    pub fn up(&self, source: &str) {
        let mut r = self.requests.lock();
        r.owners.remove(source);
        if r.owners.is_empty() {
            let _ = self.tx.try_send(Command::Stop(r.generation));
        }
    }
    pub fn cancel(&self) {
        let id = {
            let mut r = self.requests.lock();
            if r.owners.is_empty() {
                return;
            }
            r.owners.clear();
            let id = r.generation;
            r.generation = r.generation.wrapping_add(1);
            id
        };
        let _ = self.tx.try_send(Command::Stop(id));
    }
}
impl Drop for Voice {
    fn drop(&mut self) {
        self.requests.lock().owners.clear();
        let _ = self.tx.send(Command::Quit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn current(r: &Mutex<Requests>, id: u64) -> bool {
    let r = r.lock();
    r.generation == id && !r.owners.is_empty()
}
struct Queue {
    samples: VecDeque<i16>,
    ready: bool,
    socket: UdpSocket,
}
impl Queue {
    fn push(&mut self, samples: &[i16]) {
        self.samples.extend(samples);
        if !self.ready {
            while self.samples.len() > 12000 {
                self.samples.pop_front();
            }
            return;
        }
        while !self.samples.is_empty() {
            let n = self.samples.len().min(960);
            let bytes: Vec<_> = self.samples.drain(..n).flat_map(i16::to_le_bytes).collect();
            let _ = self.socket.send(&bytes);
        }
    }
}
struct Capture {
    _stream: cpal::Stream,
    queue: Arc<Mutex<Queue>>,
    name: String,
}
impl Capture {
    fn start(
        cfg: &HisenseConfig,
        peak: Arc<AtomicU32>,
        frames: Arc<AtomicU64>,
    ) -> Result<Self, String> {
        if !crate::audio::pcm_router::audio_router_ready() {
            return Err("语音路由未就绪，请在首页检查虚拟声卡与语音路由".into());
        }
        let devices = cpal::default_host()
            .input_devices()
            .map_err(|e| e.to_string())?;
        let device = devices
            .into_iter()
            .find(|d| {
                d.name().ok().is_some_and(|n| match &cfg.microphone_name {
                    Some(wanted) => n == *wanted,
                    None => {
                        let n = n.to_ascii_lowercase();
                        n.contains("audio-hid") || n.contains("hisense") || n.contains("drf3u65")
                    }
                })
            })
            .ok_or("未找到海信 USB 麦克风；请在海信语音设置中选择接收器的输入设备")?;
        let name = device.name().map_err(|e| e.to_string())?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let config: cpal::StreamConfig = supported.clone().into();
        let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        socket
            .connect(("127.0.0.1", crate::audio::pcm_router::DEFAULT_PCM_PORT))
            .map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let _ = socket.send(b"CLEAR");
        let queue = Arc::new(Mutex::new(Queue {
            samples: VecDeque::new(),
            ready: false,
            socket,
        }));
        let channels = config.channels as usize;
        let rate = config.sample_rate.0;
        let gain = 10f32.powf(cfg.microphone_gain_db / 20.);
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(
                &device,
                &config,
                channels,
                rate,
                gain,
                queue.clone(),
                peak,
                frames,
            ),
            cpal::SampleFormat::I16 => build::<i16>(
                &device,
                &config,
                channels,
                rate,
                gain,
                queue.clone(),
                peak,
                frames,
            ),
            cpal::SampleFormat::U16 => build::<u16>(
                &device,
                &config,
                channels,
                rate,
                gain,
                queue.clone(),
                peak,
                frames,
            ),
            _ => Err("接收器音频格式暂不支持".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            queue,
            name,
        })
    }
    fn open_gate(&self) {
        let mut q = self.queue.lock();
        q.ready = true;
        q.push(&[]);
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self._stream.pause();
        let mut q = self.queue.lock();
        q.ready = false;
        q.samples.clear();
        let _ = q.socket.send(b"END");
    }
}
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    rate: u32,
    gain: f32,
    queue: Arc<Mutex<Queue>>,
    peak: Arc<AtomicU32>,
    frames: Arc<AtomicU64>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let mut resampler = Resampler::new(rate);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                use cpal::Sample;
                let mono: Vec<f32> = data
                    .chunks_exact(channels)
                    .map(|frame| {
                        frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32
                            * gain
                    })
                    .collect();
                peak.store(
                    mono.iter()
                        .map(|x| x.abs())
                        .fold(0., f32::max)
                        .min(1.)
                        .to_bits(),
                    Ordering::Relaxed,
                );
                frames.fetch_add(mono.len() as u64, Ordering::Relaxed);
                let pcm = resampler.convert(&mono);
                queue.lock().push(&pcm);
            },
            move |e| log::warn!("HISENSE VOICE audio input error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}
struct Resampler {
    pending: Vec<f32>,
    position: u64,
    step: u64,
}
impl Resampler {
    fn new(rate: u32) -> Self {
        Self {
            pending: Vec::new(),
            position: 0,
            step: rate as u64,
        }
    }
    fn convert(&mut self, input: &[f32]) -> Vec<i16> {
        self.pending.extend_from_slice(input);
        let mut out = Vec::new();
        while self.position + 48000 < self.pending.len() as u64 * 48000 {
            let i = (self.position / 48000) as usize;
            let frac = (self.position % 48000) as f32 / 48000.;
            let sample = self.pending[i] * (1. - frac) + self.pending[i + 1] * frac;
            out.push((sample.clamp(-1., 1.) * 32767.).round() as i16);
            self.position += self.step;
        }
        let n = ((self.position / 48000) as usize).min(self.pending.len().saturating_sub(1));
        self.pending.drain(..n);
        self.position -= n as u64 * 48000;
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampling_preserves_continuity_across_callbacks() {
        let samples: Vec<_> = (0..160).map(|x| x as f32 / 160.).collect();
        let mut a = Resampler::new(16000);
        let whole = a.convert(&samples);
        let mut b = Resampler::new(16000);
        let mut pieces = b.convert(&samples[..71]);
        pieces.extend(b.convert(&samples[71..]));
        assert_eq!(whole, pieces);
        assert!(whole.len() > 470);
    }
}
