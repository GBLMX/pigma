use serde::{Deserialize, Serialize};

use crate::utils::{GradientPreset, deserialize_optional, terminal::ImageProtocolChoice};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayoutType {
    #[default]
    Default,
    Modern,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerbarVisible {
    pub cover: bool,
    pub volume: bool,
    pub mode_icon: bool,
    pub spinner: bool,
    /// Frequency bars of what is playing, drawn on the layout's spare row.
    pub visualizer: bool,
    /// Dominant-pitch readout (note + frequency), sharing that row with the bars.
    pub pitch: bool,
}

impl Default for PlayerbarVisible {
    fn default() -> Self {
        Self {
            cover: true,
            volume: true,
            mode_icon: true,
            spinner: true,
            visualizer: false,
            pitch: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerbarConfig {
    #[serde(default = "default_pb_filled_symbol")]
    pub filled_symbol: String,
    #[serde(default = "default_pb_unfilled_symbol")]
    pub unfilled_symbol: String,
    #[serde(default = "default_pb_filled_color")]
    pub filled_color: String,
    #[serde(default = "default_pb_unfilled_color")]
    pub unfilled_color: String,
    #[serde(default = "default_pb_unfilled_color_cached")]
    pub unfilled_color_cached: String,
    /// Progress bar gradient preset. An empty string or unknown name disables the gradient.
    #[serde(default, deserialize_with = "deserialize_optional")]
    pub gradient_preset: Option<GradientPreset>,
    /// How covers are drawn. `auto` asks the terminal and trusts its answer; force
    /// `kitty`, `iterm2`, `sixel` or `halfblocks` where the detection cannot know
    /// better (tmux with `allow-passthrough`, a multiplexer, an ssh hop).
    #[serde(default)]
    pub image_protocol: ImageProtocolChoice,
    #[serde(default)]
    pub layout: LayoutType,
    #[serde(default)]
    pub visible: PlayerbarVisible,
}

fn default_pb_filled_symbol() -> String {
    "━".into()
}
fn default_pb_unfilled_symbol() -> String {
    "─".into()
}
fn default_pb_filled_color() -> String {
    "accent".into()
}
fn default_pb_unfilled_color() -> String {
    "text".into()
}
fn default_pb_unfilled_color_cached() -> String {
    "error".into()
}

impl Default for PlayerbarConfig {
    fn default() -> Self {
        Self {
            filled_symbol: default_pb_filled_symbol(),
            unfilled_symbol: default_pb_unfilled_symbol(),
            filled_color: default_pb_filled_color(),
            unfilled_color: default_pb_unfilled_color(),
            unfilled_color_cached: default_pb_unfilled_color_cached(),
            gradient_preset: None,
            image_protocol: ImageProtocolChoice::default(),
            layout: LayoutType::default(),
            visible: PlayerbarVisible::default(),
        }
    }
}
