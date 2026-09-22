//! Bit-perfect output: WASAPI in exclusive mode.
//!
//! Everything up to here is a `rodio`/cpal stream, and shared mode means the system mixer gets
//! between the chain and the device: it resamples to whatever the mixer runs at and applies its
//! own volume. Exclusive mode hands the device's buffer straight to this process — nothing
//! between the chain and the DAC — at the price of owning the device while it plays, and of
//! having to feed it exactly the format it agreed to.
//!
//! Hence the two halves: [`negotiate`] turns a preferred rate into the format the device will
//! actually take, and [`ExclusivePlayer`] owns the stream and pulls from the chain. A player
//! that cannot open the device in exclusive mode reports why and the caller keeps the shared
//! path, rather than failing the track.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

use rodio::Source;

/// The formats the exclusive path can feed, in the order it prefers them.
///
/// Float first: the chain works in `f32` and the device almost always takes it natively, so the
/// bit-perfect case needs no conversion at all. 16-bit PCM is the fallback for the devices that
/// only accept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeviceSample {
    Float32,
    Int16,
}

/// The format the device agreed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Format {
    pub rate: u32,
    pub channels: u16,
    pub sample: DeviceSample,
}

impl DeviceSample {
    fn bytes(self) -> usize {
        match self {
            DeviceSample::Float32 => 4,
            DeviceSample::Int16 => 2,
        }
    }
}

/// Encode interleaved `f32` samples the way the device asked for them.
///
/// This is the only place the samples change shape, and for a float device it is a copy: the
/// bit-perfect path is a `memcpy` of what the chain produced.
pub(super) fn encode(samples: &[f32], format: Format, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * format.sample.bytes());
    match format.sample {
        DeviceSample::Float32 => {
            for sample in samples {
                out.extend_from_slice(&sample.to_le_bytes());
            }
        }
        DeviceSample::Int16 => {
            for sample in samples {
                // Clamp before scaling: a sample outside [-1, 1] would wrap around into the
                // opposite sign, which is a click rather than a loud passage.
                let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                out.extend_from_slice(&scaled.to_le_bytes());
            }
        }
    }
}

/// The rates to offer the device, in the order to try them.
///
/// The track's own rate comes first: a device that takes it needs no conversion at all, which
/// is as close to a straight wire as a chain gets. The device's own rate is the fallback — it is
/// what the mixer is running right now, so it is the most likely to be accepted when the first
/// attempt is not.
pub(super) fn candidate_rates(device_rate: u32, preferred_rate: u32) -> Vec<u32> {
    let mut rates = Vec::with_capacity(2);
    for rate in [preferred_rate, device_rate] {
        if rate > 0 && !rates.contains(&rate) {
            rates.push(rate);
        }
    }
    rates
}

/// Ask the device what it will take, at the rate the track would prefer.
///
/// This opens and closes the device: exclusive mode can only be asked by trying, and the answer
/// is what the caller needs before it can decide whether to convert the samples at all. The
/// device is reopened when the track actually starts.
pub(super) fn probe(preferred_rate: u32) -> Result<Format, String> {
    #[cfg(windows)]
    {
        imp::probe(preferred_rate)
    }
    #[cfg(not(windows))]
    {
        let _ = preferred_rate;
        Err("独占输出目前只在 Windows 上实现".to_string())
    }
}

/// What the playing thread and the player agree on.
#[derive(Debug)]
struct Shared {
    /// Set to stop: the thread returns and closes the device.
    stop: AtomicBool,
    /// Set while paused. The device keeps being fed — with silence — because an exclusive
    /// stream that is simply not written to underruns and clicks.
    paused: AtomicBool,
    /// Linear volume, as `f32` bits: the app's volume control, applied here rather than by a
    /// mixer that exclusive mode bypasses.
    volume: AtomicU32,
    /// Frames handed to the device, which is the play position.
    frames: AtomicU64,
    /// Set when the source ended and the tail has been flushed.
    finished: AtomicBool,
    rate: u32,
}

impl Shared {
    fn new(rate: u32, volume: f32) -> Self {
        Self {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            volume: AtomicU32::new(volume.to_bits()),
            frames: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            rate,
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

/// An exclusive output stream, playing one source.
pub(super) struct ExclusivePlayer {
    shared: Arc<Shared>,
    #[cfg(windows)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ExclusivePlayer {
    /// Start playing `source` through an exclusive device, in `format`.
    ///
    /// `format` is what [`probe`] reported: exclusive mode fixes the format before a single
    /// sample is converted, so the caller asks the device first and builds the chain for the
    /// answer.
    pub(super) fn start(
        source: Box<dyn Source<Item = f32> + Send>,
        format: Format,
        volume: f32,
    ) -> Result<Self, String> {
        #[cfg(windows)]
        {
            imp::start(source, format, volume)
        }
        #[cfg(not(windows))]
        {
            let _ = (source, format, volume);
            Err("独占输出目前只在 Windows 上实现".to_string())
        }
    }

    pub(super) fn pause(&self) {
        self.shared.paused.store(true, Ordering::Relaxed);
    }

    pub(super) fn resume(&self) {
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    pub(super) fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }

    /// Where the device is, in the units the rest of the player counts in.
    pub(super) fn position(&self) -> std::time::Duration {
        let frames = self.shared.frames.load(Ordering::Relaxed);
        std::time::Duration::from_secs_f64(frames as f64 / f64::from(self.shared.rate.max(1)))
    }

    pub(super) fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    /// Whether the source has ended and the device has been fed its tail.
    pub(super) fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }

    pub(super) fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for ExclusivePlayer {
    fn drop(&mut self) {
        self.stop();
        #[cfg(windows)]
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Windows implementation: the device, the stream, and the thread that feeds it.
#[cfg(windows)]
mod imp {
    use super::*;

    use wasapi::{
        AudioClient, AudioRenderClient, Device, DeviceEnumerator, Direction, SampleType,
        StreamMode, WaveFormat, initialize_mta,
    };

    /// One pull's worth of frames. The device is asked how much it wants each time; this is
    /// just the size of the working buffer.
    const CHUNK_FRAMES: usize = 4096;

    /// The device period offered to exclusive mode, in 100 ns units: 10 ms. Smaller periods
    /// are offered only if the device insists on them.
    const PERIOD_HNS: i64 = 100_000;

    /// Convert the device's own mixer format into the shape this module works in.
    fn describe(format: &WaveFormat) -> Option<Format> {
        let sample = match format.get_subformat().ok()? {
            SampleType::Float => DeviceSample::Float32,
            SampleType::Int => {
                if format.get_bitspersample() <= 16 {
                    DeviceSample::Int16
                } else {
                    return None;
                }
            }
        };
        Some(Format {
            rate: format.get_samplespersec(),
            channels: format.get_nchannels(),
            sample,
        })
    }

    fn wave_format(format: Format) -> WaveFormat {
        let (store_bits, valid_bits, sample) = match format.sample {
            DeviceSample::Float32 => (32, 32, SampleType::Float),
            DeviceSample::Int16 => (16, 16, SampleType::Int),
        };
        WaveFormat::new(
            store_bits,
            valid_bits,
            &sample,
            format.rate as usize,
            usize::from(format.channels),
            None,
        )
    }

    /// The format the device will take, without keeping the device open.
    ///
    /// This runs on a thread of its own rather than on the caller's: WASAPI wants an
    /// MTA-initialized thread, and by the time a track starts the caller's thread may already
    /// be in another apartment (`RPC_E_CHANGED_MODE`).
    pub(super) fn probe(preferred_rate: u32) -> Result<Format, String> {
        std::thread::Builder::new()
            .name("exclusive-probe".to_string())
            .spawn(move || {
                let (client, format, _event) = open(preferred_rate)?;
                let _ = client.stop_stream();
                Ok(format)
            })
            .map_err(|e| format!("无法启动探测线程: {e}"))?
            .join()
            .map_err(|_| "探测线程崩溃".to_string())?
    }

    /// Open the default render device in exclusive mode, at the rate the device will take.
    fn open(preferred_rate: u32) -> Result<(AudioClient, Format, wasapi::Handle), String> {
        initialize_mta().ok().map_err(|e| e.to_string())?;

        let enumerator = DeviceEnumerator::new().map_err(|e| format!("枚举设备失败: {e}"))?;
        let device: Device = enumerator
            .get_default_device(&Direction::Render)
            .map_err(|e| format!("没有默认输出设备: {e}"))?;
        let mut client: AudioClient = device
            .get_iaudioclient()
            .map_err(|e| format!("获取音频客户端失败: {e}"))?;

        let mix_raw = client
            .get_mixformat()
            .map_err(|e| format!("读取设备格式失败: {e}"))?;
        log::debug!(
            "exclusive: device mix {} Hz, {} ch, {} bit, subformat {:?}, mask {:#x}",
            mix_raw.get_samplespersec(),
            mix_raw.get_nchannels(),
            mix_raw.get_bitspersample(),
            mix_raw.get_subformat(),
            mix_raw.get_dwchannelmask(),
        );
        let mix = describe(&mix_raw).ok_or_else(|| "设备格式不支持独占输出".to_string())?;

        // Exclusive mode is fussy in ways plain `IsFormatSupported` does not capture — most of
        // all the channel mask, which a stereo device may report as 0 and then refuse. The
        // helper tries the format as given, as a plain WAVEFORMATEX, and with each plausible
        // mask, and hands back the one that worked: that is the one to open with.
        let mut chosen: Option<(WaveFormat, Format)> = None;
        for rate in candidate_rates(mix.rate, preferred_rate) {
            let raw = wave_format(Format { rate, ..mix });
            log::debug!(
                "exclusive: trying {} Hz -> {:?}",
                rate,
                client
                    .is_supported_exclusive_with_quirks(&raw)
                    .map(|f| (f.get_samplespersec(), f.get_nchannels()))
            );
            if let Ok(working) = client.is_supported_exclusive_with_quirks(&raw)
                && let Some(described) = describe(&working)
            {
                chosen = Some((working, described));
                break;
            }
        }
        let Some((wave, format)) = chosen else {
            // The device refused every candidate, including its own mixer format. That is not
            // a format problem: it is the device refusing exclusive mode as such, which on
            // Windows means either another process is holding it, or the endpoint's
            // "allow applications to take exclusive control" is switched off.
            return Err(
                "设备不允许独占模式（可能被其他程序占用，或设备属性里未允许独占控制）".to_string(),
            );
        };

        client
            .initialize_client(
                &wave,
                &Direction::Render,
                &StreamMode::EventsExclusive {
                    period_hns: PERIOD_HNS,
                },
            )
            .map_err(|e| format!("独占初始化失败: {e}"))?;
        let event = client
            .set_get_eventhandle()
            .map_err(|e| format!("等待设备事件失败: {e}"))?;
        client
            .start_stream()
            .map_err(|e| format!("启动独占流失败: {e}"))?;

        Ok((client, format, event))
    }

    pub(super) fn start(
        mut source: Box<dyn Source<Item = f32> + Send>,
        format: Format,
        volume: f32,
    ) -> Result<ExclusivePlayer, String> {
        // The device is opened *inside* the playing thread: the COM handles are not `Send`, and
        // an exclusive stream is a contract with one thread anyway. `probe` has already told the
        // caller which format to build the chain for; here the same question is asked for real.
        let shared = Arc::new(Shared::new(format.rate, volume));
        let thread_shared = Arc::clone(&shared);

        let thread = std::thread::Builder::new()
            .name("exclusive-audio".to_string())
            .spawn(move || {
                let (client, actual, event) = match open(format.rate) {
                    Ok(opened) => opened,
                    Err(error) => {
                        log::error!("独占输出打开失败: {error}");
                        thread_shared.finished.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                if actual != format {
                    // The device changed its mind between the probe and the start (another
                    // process took it, or it was switched): feeding it the wrong rate would be
                    // worse than not playing at all.
                    log::error!("设备在探测与启动之间改变了格式: {format:?} -> {actual:?}");
                    let _ = client.stop_stream();
                    thread_shared.finished.store(true, Ordering::Relaxed);
                    return;
                }
                let render: AudioRenderClient = match client.get_audiorenderclient() {
                    Ok(render) => render,
                    Err(error) => {
                        log::error!("独占输出渲染客户端失败: {error}");
                        let _ = client.stop_stream();
                        thread_shared.finished.store(true, Ordering::Relaxed);
                        return;
                    }
                };
                let mut encoded: Vec<u8> = Vec::new();
                let mut chunk: Vec<f32> = vec![0.0; CHUNK_FRAMES * usize::from(format.channels)];
                let mut drained = false;
                while !thread_shared.stop.load(Ordering::Relaxed) {
                    let Ok(free) = client.get_available_space_in_frames() else {
                        break;
                    };
                    if free == 0 {
                        // Wait for the device to have room. The timeout is only a safety net:
                        // the event fires when the buffer drains.
                        if event.wait_for_event(500).is_err() {
                            continue;
                        }
                        continue;
                    }

                    let frames = (free as usize).min(CHUNK_FRAMES);
                    let wanted = frames * usize::from(format.channels);

                    if thread_shared.paused.load(Ordering::Relaxed) {
                        chunk[..wanted].fill(0.0);
                    } else {
                        let mut filled = 0;
                        while filled < wanted {
                            match source.next() {
                                Some(sample) => {
                                    chunk[filled] = sample;
                                    filled += 1;
                                }
                                None => break,
                            }
                        }
                        if filled == 0 {
                            if drained {
                                thread_shared.finished.store(true, Ordering::Relaxed);
                                break;
                            }
                            // The tail: keep the device fed with silence for one buffer so the
                            // last samples leave the device instead of being cut off.
                            drained = true;
                            chunk[..wanted].fill(0.0);
                        } else {
                            chunk[filled..wanted].fill(0.0);
                        }
                        let volume = thread_shared.volume();
                        for sample in &mut chunk[..wanted] {
                            *sample *= volume;
                        }
                    }

                    encode(&chunk[..wanted], format, &mut encoded);
                    if render.write_to_device(frames, &encoded, None).is_err() {
                        break;
                    }
                    thread_shared
                        .frames
                        .fetch_add(frames as u64, Ordering::Relaxed);
                }
                let _ = client.stop_stream();
            })
            .map_err(|e| format!("无法启动独占播放线程: {e}"))?;

        Ok(ExclusivePlayer {
            shared,
            thread: Some(thread),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(sample: DeviceSample) -> Format {
        Format {
            rate: 48_000,
            channels: 2,
            sample,
        }
    }

    /// A float device gets the chain's samples back byte for byte: that copy is the whole
    /// point of the exclusive path.
    #[test]
    fn a_float_device_gets_the_samples_unchanged() {
        let mut out = Vec::new();
        encode(&[0.0, 0.5, -0.25], format(DeviceSample::Float32), &mut out);
        assert_eq!(out.len(), 12);
        assert_eq!(f32::from_le_bytes(out[4..8].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(out[8..12].try_into().unwrap()), -0.25);
    }

    /// A device that only takes 16-bit PCM gets a clamped conversion: a sample outside the
    /// range must not wrap into the opposite sign, which would be a click.
    #[test]
    fn a_sixteen_bit_device_gets_clamped_pcm() {
        let mut out = Vec::new();
        encode(
            &[1.0, -1.0, 2.0, -2.0],
            format(DeviceSample::Int16),
            &mut out,
        );
        let value = |i: usize| i16::from_le_bytes(out[i * 2..i * 2 + 2].try_into().unwrap());
        assert_eq!(value(0), i16::MAX);
        assert_eq!(value(1), -i16::MAX, "full scale maps to the negative peak");
        assert_eq!(
            value(2),
            i16::MAX,
            "beyond full scale clamps, it does not wrap"
        );
        assert_eq!(value(3), -i16::MAX);
    }

    /// The track's rate is offered first — taking it means the chain converts nothing — and the
    /// device's own rate is the fallback. Asking for the same rate twice asks once.
    #[test]
    fn the_tracks_rate_is_offered_before_the_devices_own() {
        assert_eq!(candidate_rates(48_000, 44_100), vec![44_100, 48_000]);
        assert_eq!(candidate_rates(48_000, 48_000), vec![48_000]);
        assert_eq!(
            candidate_rates(48_000, 0),
            vec![48_000],
            "a track with no rate of its own leaves the device's"
        );
    }

    /// Opening a real device in exclusive mode: the one check this environment cannot make
    /// for the listener, so it is a test that asks for the device explicitly.
    ///
    /// Windows only: exclusive output is WASAPI's, and `probe` says exactly that everywhere
    /// else. Without the gate the benchmark run `cargo test --release --lib -- --ignored`
    /// — the one the perf numbers are reproduced with — fails on Linux for a platform
    /// feature rather than for the code under it.
    #[cfg(windows)]
    #[test]
    #[ignore = "opens the default output device in exclusive mode"]
    fn the_default_device_opens_exclusively() {
        let source = rodio::buffer::SamplesBuffer::new(
            std::num::NonZero::new(1).unwrap(),
            std::num::NonZero::new(48_000).unwrap(),
            vec![0.0f32; 4_800],
        );
        let format = match probe(48_000) {
            Ok(format) => format,
            Err(error) => panic!("独占输出打不开: {error}"),
        };
        let player = match ExclusivePlayer::start(Box::new(source), format, 1.0) {
            Ok(player) => player,
            Err(error) => panic!("独占输出打不开: {error}"),
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            player.position() > std::time::Duration::ZERO,
            "设备没有消费任何帧"
        );
        drop(player);
    }
}
