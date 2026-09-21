//! Cross-cutting helpers: string formatting, gradients, filesystem paths,
//! terminal setup, and time formatting.

pub mod format;
pub mod gradient;
pub mod named;
pub mod path;
pub mod terminal;
pub mod time;

pub use gradient::{GradientPreset, deserialize_optional, gradient_color};
pub use named::Named;
pub use path::{boxpigma_cache_dir, boxpigma_config_dir, sanitize_filename};
pub use time::{clock_time, format_duration, format_duration_into, local_timestamp};
