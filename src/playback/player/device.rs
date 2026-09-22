//! Opening the output device, and the one platform shape it has.
//!
//! Two things put this behind its own module rather than inline in the player loop: the device
//! is opened once and kept across songs, and on Linux the open is noisy — ALSA writes to stderr
//! while the device is set up, which lands in the middle of the TUI's frame. The Linux half of
//! that, the stderr guard, is `imp` below; [`create_sink`] is the portable entry point.

use std::sync::Arc;

use super::{DeviceHealth, stream_error_callback};

/// Open the audio device while suppressing ALSA stderr noise (Linux only).
/// The returned sink should be kept alive across songs so the device is
/// opened only once — until it is lost and must be rebuilt.
pub(super) fn create_sink(
    health: Arc<DeviceHealth>,
) -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    #[cfg(target_os = "linux")]
    {
        // The guard has to live across `open_sink_impl`: `let _ = StderrGuard::new()?` would
        // drop it at the end of that statement, which is exactly the window it exists for —
        // so it was silencing nothing while the ALSA noise it was written for happened.
        //
        // Failing to silence stderr is also not a reason to refuse to open the device:
        // ALSA chatter in the log beats no audio at all, and `?` here turned a cosmetic
        // failure into `DeviceSinkError::NoDevice`.
        let _silencer = imp::StderrGuard::new().map_err(|e| {
            log::warn!("failed to silence ALSA on stderr: {e}");
        });
        open_sink_impl(health)
    }
    #[cfg(not(target_os = "linux"))]
    {
        open_sink_impl(health)
    }
}

/// Prefer PipeWire/PulseAudio ALSA devices so system volume/mute works.
/// Falls back to the default ALSA device if not available.
fn open_sink_impl(
    health: Arc<DeviceHealth>,
) -> Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
    ))]
    {
        // Only this block talks to cpal directly; elsewhere the traits are not in scope.
        use rodio::cpal::traits::{DeviceTrait, HostTrait};

        let host = rodio::cpal::default_host();
        if let Ok(devices) = host.devices() {
            let list: Vec<_> = devices.collect();

            for name in ["pipewire", "pulse"] {
                if let Some(device) = list
                    .iter()
                    .find(|d| d.id().map(|id| id.1.as_str() == name).unwrap_or(false))
                {
                    log::info!("opening audio device: {}", name);
                    if let Ok(sink) = rodio::DeviceSinkBuilder::from_device(device.clone())
                        .map(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(8192)))
                        .map(|b| b.with_error_callback(stream_error_callback(health.clone())))
                        .and_then(|b| b.open_sink_or_fallback())
                    {
                        return Ok(sink);
                    }
                    log::warn!("failed to open {}, falling back", name);
                } else {
                    log::debug!("cpal device not found: {}", name);
                }
            }
        }
    }

    log::debug!("falling back to default audio device");
    rodio::DeviceSinkBuilder::from_default_device()
        .map(|b| b.with_error_callback(stream_error_callback(health)))
        .and_then(|b| b.open_sink_or_fallback())
}

/// The Linux half of opening the device: the guard that keeps ALSA quiet while it happens.
#[cfg(target_os = "linux")]
mod imp {
    /// RAII guard that redirects stderr to /dev/null while alive, restoring it on drop.
    /// Used to suppress ALSA noise during audio device initialization.
    pub(super) struct StderrGuard {
        saved_fd: std::os::fd::RawFd,
    }

    impl StderrGuard {
        pub(super) fn new() -> std::io::Result<Self> {
            use std::os::fd::AsRawFd;

            let stderr_fd = 2;
            let saved = unsafe { libc::dup(stderr_fd) };
            if saved < 0 {
                return Err(std::io::Error::last_os_error());
            }

            let dev_null = std::fs::File::open("/dev/null")?;
            let ret = unsafe { libc::dup2(dev_null.as_raw_fd(), stderr_fd) };
            if ret < 0 {
                unsafe { libc::close(saved) };
                return Err(std::io::Error::last_os_error());
            }

            Ok(Self { saved_fd: saved })
        }
    }

    impl Drop for StderrGuard {
        fn drop(&mut self) {
            let stderr_fd = 2;
            unsafe {
                libc::dup2(self.saved_fd, stderr_fd);
                libc::close(self.saved_fd);
            }
        }
    }
}
