use super::chain;
use std::{
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use rodio::{
    Source,
    cpal::traits::{DeviceTrait, HostTrait},
};
use tokio::sync::mpsc;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::engine::mem_rss_kb;
use super::spectrum::{self, SpectrumTap};
use crate::event::{AppEvent, Event, PlaybackEvent};

mod device;

use device::create_sink;

/// Progress ticks (~200ms each) the position may stay frozen while playing
/// before we assume the audio stream is dead (e.g. Bluetooth device removed on
/// macOS sometimes dies without firing the cpal error callback) and rebuild it.
/// Network stalls also freeze the position, but those are accompanied by recent
/// buffer-underrun errors, so they are excluded via [`UNDERRUN_FRESH_MS`].
const STALL_TICKS: u32 = 25;
/// A buffer-underrun error younger than this (ms) means the freeze is a data
/// feed problem (slow network), not a dead device — suppress the watchdog.
const UNDERRUN_FRESH_MS: u64 = 10_000;
/// Poll the system default output device name every N progress ticks (~1s).
const FOLLOW_POLL_TICKS: u32 = 5;
/// Rebuild to follow the default device only after the change is stable for
/// this many consecutive polls (~2s), avoiding flaps during BT handshakes.
const FOLLOW_DIFF_THRESHOLD: u32 = 2;

/// Shared between the stream error callback and the player loop: set when a
/// fatal stream error indicates the output device/stream must be rebuilt.
struct DeviceHealth {
    /// Fatal stream error seen — rebuild the sink as soon as possible.
    device_lost: AtomicBool,
    /// Milliseconds (relative to `epoch`) of the last underrun, used to tell
    /// network stalls apart from device death. `u64::MAX` = none.
    last_underrun_ms: AtomicU64,
    /// Shared instant the millisecond timestamps are relative to.
    epoch: Instant,
}

pub trait AudioReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AudioReader for T {}

pub type AudioInput = SharedReader;

/// A reader wrapper that can be shared between the player and the seek controller.
/// On seek, the controller locks the shared reader, seeks to the new position,
/// then creates a new decoder + player from the seeked position.
#[derive(Clone)]
pub struct SharedReader(pub Arc<Mutex<Box<dyn AudioReader + 'static>>>);

impl std::fmt::Debug for SharedReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedReader").finish_non_exhaustive()
    }
}

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(format!("mutex poisoned: {e}")))?
            .read(buf)
    }
}

impl Seek for SharedReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0
            .lock()
            .map_err(|e| std::io::Error::other(format!("mutex poisoned: {e}")))?
            .seek(pos)
    }
}

#[derive(Debug, Clone)]
pub enum ControlCmd {
    Switch(AudioInput, Option<Duration>),
    SeekTo(Duration),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
}

/// Run the audio player as a persistent blocking task. The task is spawned once
/// and stays alive across songs — new sources are fed via ControlCmd::Switch.
///
/// What the loop drives.
///
/// Shared mode goes through rodio's player, which mixes into the system's stream; exclusive mode
/// owns the device itself. The loop only ever asks them the same few questions — where the track
/// is, whether it is paused, whether it has ended — so they meet here.
enum Output {
    Shared(rodio::Player),
    Exclusive(super::exclusive::ExclusivePlayer),
}

impl Output {
    fn position(&self) -> Duration {
        match self {
            Self::Shared(player) => player.get_pos(),
            Self::Exclusive(player) => player.position(),
        }
    }

    fn is_paused(&self) -> bool {
        match self {
            Self::Shared(player) => player.is_paused(),
            Self::Exclusive(player) => player.is_paused(),
        }
    }

    /// Whether the source has run out. rodio reports an empty queue; the exclusive player
    /// reports that it has flushed the tail and stopped feeding the device.
    fn is_finished(&self) -> bool {
        match self {
            Self::Shared(player) => player.empty() && !player.is_paused(),
            Self::Exclusive(player) => player.is_finished(),
        }
    }

    fn stop(&self) {
        match self {
            Self::Shared(player) => player.stop(),
            Self::Exclusive(player) => player.stop(),
        }
    }

    fn pause(&self) {
        match self {
            Self::Shared(player) => player.pause(),
            Self::Exclusive(player) => player.pause(),
        }
    }

    fn resume(&self) {
        match self {
            Self::Shared(player) => player.play(),
            Self::Exclusive(player) => player.resume(),
        }
    }

    fn set_volume(&self, volume: f32) {
        match self {
            Self::Shared(player) => player.set_volume(volume),
            Self::Exclusive(player) => player.set_volume(volume),
        }
    }
}

/// Hand the source to an exclusive device, or hand it back with the reason.
///
/// The source comes back on every failure — the device refused the format, it is taken by
/// another process, the thread could not start — so the caller can fall back to the shared
/// path with the samples it already has, rather than failing the track.
fn start_exclusive(
    dsp: &crate::config::AudioConfig,
    source: Box<dyn Source<Item = f32> + Send>,
    volume: f32,
) -> Result<super::exclusive::ExclusivePlayer, (Box<dyn Source<Item = f32> + Send>, String)> {
    let decoded_rate = source.sample_rate().get();
    let format = match super::exclusive::probe(decoded_rate) {
        Ok(format) => format,
        Err(error) => return Err((source, error)),
    };
    // The chain is built for what the device agreed to: in exclusive mode there is no mixer
    // behind us to convert anything.
    let built = chain::build(source, format.rate, dsp);
    match super::exclusive::ExclusivePlayer::start(built, format, volume) {
        Ok(player) => Ok(player),
        Err(error) => Err((
            Box::new(silence()) as Box<dyn Source<Item = f32> + Send>,
            error,
        )),
    }
}

/// A source that ends immediately. Only used where a failed exclusive start has already taken
/// the samples with it and the caller has been told why.
fn silence() -> rodio::buffer::SamplesBuffer {
    rodio::buffer::SamplesBuffer::new(
        std::num::NonZero::new(1).expect("one channel"),
        std::num::NonZero::new(48_000).expect("48 kHz"),
        Vec::new(),
    )
}

/// The rate the open device runs at, which is what the file's rate has to be converted to.
fn device_rate(sink: &rodio::MixerDeviceSink) -> Option<u32> {
    Some(sink.config().sample_rate().get())
}

/// The audio device sink is owned by this task (created lazily on first
/// playback) so it can be rebuilt when the output device goes away: Bluetooth
/// headsets being disconnected/reconnected invalidate the old stream, and the
/// only fix is to re-enumerate devices and open the current default again.
pub(super) fn run(
    initial_reader: SharedReader,
    initial_seek_time: Option<Duration>,
    initial_volume: f32,
    dsp: crate::config::AudioConfig,
    event_tx: mpsc::UnboundedSender<Event>,
    control_rx: std::sync::mpsc::Receiver<ControlCmd>,
) {
    tokio::task::spawn_blocking(move || {
        let progress_interval = Duration::from_millis(200);
        let mut reader = initial_reader;
        let mut total_duration: Option<Duration> = None;
        let mut seek_offset: Duration = Duration::default();
        let mut volume = initial_volume;
        // Owned by this task; recreated whenever the output device is lost.
        let mut sink: Option<rodio::MixerDeviceSink> = None;
        let health = Arc::new(DeviceHealth {
            device_lost: AtomicBool::new(false),
            last_underrun_ms: AtomicU64::new(u64::MAX),
            epoch: Instant::now(),
        });
        // Watchdog state: position frozen while playing => dead stream.
        let mut last_pos = Duration::default();
        let mut stall_ticks: u32 = 0;
        let mut player: Option<Output> = None;
        // Default-device following state (see FOLLOW_POLL_TICKS).
        let mut sink_dev_id: Option<String> = None;
        let mut follow_poll: u32 = 0;
        let mut follow_diff: u32 = 0;
        // Set once the device could not be opened. Without it, `create_sink` failing was
        // completely silent: the UI showed a track that had "started" and never advanced,
        // and the only trace was a debug line in the log. Reported once per failure rather
        // than on every command that retries.
        let mut sink_failure_reported = false;

        macro_rules! ensure_sink {
            () => {{
                if sink.is_none()
                    && let Ok(mut s) = create_sink(health.clone())
                {
                    s.log_on_drop(false);
                    sink = Some(s);
                    sink_dev_id = current_default_id();
                    sink_failure_reported = false;
                } else if sink.is_none() && !sink_failure_reported {
                    sink_failure_reported = true;
                    log::error!("no audio output device: playback cannot start");
                    // A toast and not `PlaybackEvent::Error`: the error path walks the
                    // per-song retry/skip state machine, and "no sound card" is not a
                    // reason to skip the song or to stop the queue.
                    let _ = event_tx.send(
                        AppEvent::Toast("没有可用的音频输出设备，播放无法开始".into()).into(),
                    );
                }
                sink.is_some()
            }};
        }

        macro_rules! start_playback {
            ($seek_time:expr) => {{
                let input = reader.clone();
                match rodio::Decoder::new(input) {
                    Ok(d) => {
                        total_duration = d.total_duration();
                        let seek_time = $seek_time;
                        let (source, offset): (Box<dyn Source<Item = f32> + Send>, Duration) =
                            if let Some(t) = seek_time {
                                let mut d = d;
                                if d.try_seek(t).is_err() {
                                    log::warn!("try_seek failed for {t:?}, starting from 0");
                                    (Box::new(d), Duration::default())
                                } else {
                                    (Box::new(d), t)
                                }
                            } else {
                                (Box::new(d), Duration::default())
                            };
                        seek_offset = offset;
                        // Convert the file's rate to the device's before anything else looks
                        // at the samples: the spectrum shows what is heard, and so does the
                        // volume the device gets. A file already at the device's rate is not
                        // touched at all, which is the bit-perfect path.
                        if dsp.exclusive {
                            match start_exclusive(&dsp, source, volume) {
                                Ok(player) => {
                                    last_pos = Duration::default();
                                    stall_ticks = 0;
                                    Some(Output::Exclusive(player))
                                }
                                Err((_source, error)) => {
                                    // The shared path is still there, and a track that plays
                                    // through the mixer beats a track that does not play.
                                    log::warn!("独占输出失败，改用共享模式: {error}");
                                    let _ = event_tx.send(
                                        AppEvent::Toast(format!(
                                            "独占输出失败，改用共享模式: {error}"
                                        ))
                                        .into(),
                                    );
                                    None
                                }
                            }
                        } else {
                            let source = match sink.as_ref().and_then(device_rate) {
                                Some(rate) => chain::build(source, rate, &dsp),
                                None => source,
                            };
                            let p = rodio::Player::connect_new(
                                &sink.as_ref().expect("sink ensured").mixer().clone(),
                            );
                            p.set_volume(volume);
                            p.append(SpectrumTap::new(source, spectrum::buffer()));
                            last_pos = Duration::default();
                            stall_ticks = 0;
                            Some(Output::Shared(p))
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(PlaybackEvent::Error(format!("decode: {e}")).into());
                        None
                    }
                }
            }};
        }

        // Drop the current player/sink and reopen the (current default) audio
        // device. If `resume_at`/`was_paused` are provided, playback continues
        // from there on the fresh stream.
        macro_rules! rebuild_sink {
            ($resume_at:expr, $was_paused:expr) => {{
                let resume_at: Option<Duration> = $resume_at;
                let was_paused: bool = $was_paused;
                drop(player.take());
                drop(sink.take());
                match create_sink(health.clone()) {
                    Ok(mut s) => {
                        s.log_on_drop(false);
                        sink = Some(s);
                        sink_dev_id = current_default_id();
                        health.device_lost.store(false, Ordering::Relaxed);
                        stall_ticks = 0;
                        log::info!(
                            "音频设备已重建{}",
                            resume_at
                                .map(|t| format!("，从 {t:?} 继续"))
                                .unwrap_or_default()
                        );
                        if let Some(t) = resume_at {
                            let _ = reader.0.lock().map(|mut r| r.seek(SeekFrom::Start(0)));
                            player = start_playback!(Some(t));
                        }
                        if was_paused && let Some(ref p) = player {
                            p.pause();
                        }
                    }
                    Err(e) => {
                        // Keep device_lost set so the loop retries next tick
                        // (e.g. device still reconnecting).
                        log::warn!("重建音频设备失败，稍后重试: {e}");
                    }
                }
            }};
        }

        // Lazy-init the audio sink before first playback.
        if ensure_sink!() {
            // Initial playback.
            player = start_playback!(initial_seek_time);
        }

        loop {
            match control_rx.recv_timeout(progress_interval) {
                Ok(cmd) => match cmd {
                    ControlCmd::Switch(input, seek_time) => {
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        reader = input;
                        if ensure_sink!() {
                            player = start_playback!(seek_time);
                        } else {
                            player = None;
                        }
                    }
                    ControlCmd::SeekTo(seek_time) => {
                        if player.is_none() {
                            continue;
                        }
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        let _ = reader.0.lock().map(|mut r| r.seek(SeekFrom::Start(0)));
                        if !ensure_sink!() {
                            player = None;
                            continue;
                        }
                        player = start_playback!(Some(seek_time));
                    }
                    ControlCmd::Stop => {
                        if let Some(ref p) = player {
                            p.stop();
                        }
                        drop(player.take());
                        reader = SharedReader(Arc::new(Mutex::new(Box::new(std::io::empty()))));
                        total_duration = None;
                        seek_offset = Duration::default();
                        #[cfg(all(target_os = "linux", target_env = "gnu"))]
                        log::info!("[HEAP] after ControlCmd::Stop: {} kB", mem_rss_kb());
                    }
                    ControlCmd::Pause => {
                        if let Some(ref p) = player {
                            p.pause();
                        }
                    }
                    ControlCmd::Resume => {
                        if let Some(ref p) = player {
                            p.resume();
                        }
                    }
                    ControlCmd::SetVolume(v) => {
                        volume = v;
                        if let Some(ref p) = player {
                            p.set_volume(v);
                        }
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Fatal stream error (device removed etc.) — rebuild and resume at
            // the last known position.
            if health.device_lost.load(Ordering::Relaxed) {
                let resume_at = player.as_ref().map(|p| p.position() + seek_offset);
                let was_paused = player.as_ref().is_some_and(|p| p.is_paused());
                rebuild_sink!(resume_at, was_paused);
                continue;
            }

            // Follow system default output device changes: after a Bluetooth
            // headset reconnects the OS makes it default again, and without
            // this the stream would keep playing on the fallback (speakers)
            // forever. Rebuild only when the change is stable, and keep
            // playing at the current position on the new device.
            follow_poll += 1;
            if follow_poll >= FOLLOW_POLL_TICKS {
                follow_poll = 0;
                if sink.is_some()
                    && let Some(id) = current_default_id()
                {
                    if Some(&id) != sink_dev_id.as_ref() {
                        follow_diff += 1;
                    } else {
                        follow_diff = 0;
                    }
                    if follow_diff >= FOLLOW_DIFF_THRESHOLD {
                        log::info!(
                            "默认输出设备变化（{} -> {id}），切换音频设备",
                            sink_dev_id.as_deref().unwrap_or("?")
                        );
                        follow_diff = 0;
                        let resume_at = player.as_ref().map(|p| p.position() + seek_offset);
                        let was_paused = player.as_ref().is_some_and(|p| p.is_paused());
                        rebuild_sink!(resume_at, was_paused);
                        continue;
                    }
                }
            }

            if let Some(ref p) = player {
                if p.is_finished() {
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    log::info!(
                        "[HEAP] song finished (playback complete): {} kB",
                        mem_rss_kb()
                    );
                    let _ = event_tx.send(PlaybackEvent::Finished.into());
                    p.stop();
                    drop(player.take());
                    total_duration = None;
                    seek_offset = Duration::default();
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    log::info!("[HEAP] after player drop on finish: {} kB", mem_rss_kb());
                    continue;
                }

                if !p.is_paused() {
                    let pos = p.position() + seek_offset;
                    let _ = event_tx.send(
                        PlaybackEvent::Progress {
                            position: pos,
                            total: total_duration,
                        }
                        .into(),
                    );

                    // Watchdog: position frozen for STALL_TICKS while playing
                    // means the stream died silently (macOS sometimes removes
                    // the output device without firing the error callback).
                    // Skip when underruns were seen recently — that freeze is
                    // just the network lagging behind.
                    if !p.is_finished() {
                        let raw_pos = p.position();
                        if raw_pos == last_pos {
                            stall_ticks += 1;
                        } else {
                            stall_ticks = 0;
                            last_pos = raw_pos;
                        }
                        let underrun_ms = health.last_underrun_ms.load(Ordering::Relaxed);
                        let now_ms = underrun_now(&health.epoch);
                        let recent_underrun =
                            now_ms.saturating_sub(underrun_ms) < UNDERRUN_FRESH_MS;
                        if stall_ticks >= STALL_TICKS && !recent_underrun {
                            log::warn!(
                                "播放位置持续冻结且无网络欠载，疑似输出设备失效，重建音频流"
                            );
                            rebuild_sink!(Some(pos), false);
                        }
                    }
                }
            }
        }
    });
}

/// Milliseconds since `epoch`, matching how `DeviceHealth.last_underrun_ms`
/// timestamps are written by the stream error callback.
fn underrun_now(epoch: &std::time::Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Identifier of the current system default output device, used to detect when
/// the OS (e.g. a Bluetooth reconnection) or the user changes it. `None` when
/// no default exists right now or the id cannot be queried — treated as "no
/// change" so transient states never trigger a rebuild.
fn current_default_id() -> Option<String> {
    rodio::cpal::default_host()
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string())
}

/// rodio's default error callback prints `eprintln!("audio stream error: {err}")` straight to
/// stderr, which pollutes the TUI render in crossterm raw mode (typically triggered by a stream
/// downloading slower than playback, causing audio device buffer underruns). We replace
/// it with a callback that logs and updates [`DeviceHealth`]: transient underruns/overruns (which
/// rodio recovers from automatically) are only recorded; fatal errors (device removed,
/// stream invalidated, backend-specific device loss on WASAPI/CoreAudio/ALSA) set the
/// `device_lost` flag so the player loop rebuilds the sink.
fn stream_error_callback(
    health: Arc<DeviceHealth>,
) -> impl FnMut(rodio::cpal::StreamError) + Send + 'static + Clone {
    move |err| match err {
        rodio::cpal::StreamError::BufferUnderrun => {
            log::warn!("音频流缓冲下溢/溢出（网络可能跟不上播放速度），rodio 会自动恢复");
            health
                .last_underrun_ms
                .store(underrun_now(&health.epoch), Ordering::Relaxed);
        }
        // cpal documents both variants as "the stream must be rebuilt".
        rodio::cpal::StreamError::DeviceNotAvailable
        | rodio::cpal::StreamError::StreamInvalidated => {
            log::warn!("输出设备不可用（{err}），准备重建音频流");
            health.device_lost.store(true, Ordering::Relaxed);
        }
        other @ rodio::cpal::StreamError::BackendSpecific { .. } => {
            // Platform device-removal surfaces here: WASAPI DEVICE_INVALIDATED
            // (Windows), CoreAudio AudioUnit errors (macOS), ALSA snd_pcm
            // failures (Linux). Treat as fatal for the current stream.
            log::warn!("音频流后端错误: {other}");
            health.device_lost.store(true, Ordering::Relaxed);
        }
    }
}
