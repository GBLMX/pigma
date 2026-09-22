//! What the terminal can do, and how to talk to it.
//!
//! Four subjects, one file each behind this facade:
//!
//! - `capability` — what the terminal is: colour depth, graphics protocol, cursor shape.
//! - `color` — the colour maths those answers are built on: the palettes, and the luminance a
//!   colour works out to.
//! - `background` — the terminal's own background, with the per-platform probe under it
//!   (`background::unix`, `background::windows`).
//! - `sequences` — the escape sequences the app emits.
//!
//! Every item is re-exported here, so callers keep addressing `crate::utils::terminal::<item>`.
//! `sequences` and `background` reach each other through the items themselves (a `pub(super)`
//! helper, a child module), not through this facade.

mod background;
mod capability;
mod color;
mod sequences;

pub use background::{
    BACKGROUND, Background, BackgroundFill, BackgroundMode, parse_colorfgbg, parse_osc11_luminance,
    query_background_luminance,
};
pub use capability::{
    COLOR_MODE, ColorMode, CursorStyle, ImageProtocol, ImageProtocolChoice, choose_image_protocol,
    color_mode_from,
};
pub use color::{rgb_to_16, rgb_to_256};
pub use sequences::{
    begin_synchronized_update, disable_terminal_modes, enable_terminal_modes,
    end_synchronized_update, notify,
};

// Every `pub(crate)` item keeps its old path here, even where the re-export is not what reads
// it: `rgb_luminance` is reached through `color` by the Windows console probe, and the two
// palette tables only by the test-only cover dump in `ui::shots` — so a build without
// `--tests` finds no user for those three names.
#[allow(unused_imports)]
pub(crate) use color::{
    ANSI_16, background_from_luminance, color_luminance, palette_rgb, rgb_luminance,
};
