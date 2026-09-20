//! Logging setup: a `tracing` subscriber writing into a rolling file.
//!
//! The 135 `log::*!` call sites keep using the facade — `tracing-log` bridges `Log` records
//! into the subscriber, and `tracing_subscriber::fmt()` installs that bridge itself, so only
//! the sink changed. What the hand-written `Log` impl could not do is done here by the
//! appender: files roll daily and the oldest are pruned, so the log cannot grow without bound.

use std::path::PathBuf;

use log::Level;
use serde::{Deserialize, Serialize};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{filter::LevelFilter, fmt, fmt::time::LocalTime};

use crate::{config::Config, utils::boxpigma_config_dir};

/// Daily files kept before the appender prunes the oldest.
const LOG_FILES_KEPT: usize = 7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logger {
    pub log_level: Level,
}

fn log_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(".")
    } else {
        boxpigma_config_dir()
    }
}

impl Default for Logger {
    fn default() -> Self {
        // `Info` by default: at `Debug` the HTTP layer logs response-body previews, which
        // include account-related payloads, into the log file. Users who need the verbose
        // trace set `[logger] log_level = "DEBUG"` explicitly.
        Logger {
            log_level: Level::Info,
        }
    }
}

/// `log`'s levels and `tracing`'s filters are different types; these five are the mapping.
fn filter(level: Level) -> LevelFilter {
    match level {
        Level::Error => LevelFilter::ERROR,
        Level::Warn => LevelFilter::WARN,
        Level::Info => LevelFilter::INFO,
        Level::Debug => LevelFilter::DEBUG,
        Level::Trace => LevelFilter::TRACE,
    }
}

pub fn init_logger(config: &Config) -> color_eyre::Result<()> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("debug.log")
        .max_log_files(LOG_FILES_KEPT)
        .build(log_dir())?;

    fmt()
        .with_writer(appender)
        // The file is read with `tail` and editors, never a terminal, so no escape codes.
        .with_ansi(false)
        // Module path per line, as the hand-written format carried.
        .with_target(true)
        // Local time: the hand-written format printed local timestamps and logs are read
        // side by side with what the user was doing.
        .with_timer(LocalTime::rfc_3339())
        .with_max_level(filter(config.logger.log_level))
        .try_init()
        .map_err(|error| color_eyre::eyre::eyre!("installing the log subscriber: {error}"))?;

    Ok(())
}
