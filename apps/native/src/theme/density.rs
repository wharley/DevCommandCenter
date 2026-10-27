//! "Densidade da interface" (`dcc-density`, `html[data-density]`): the
//! composer's padding and input height and the terminal toolbar's height,
//! roomier (the default) or tighter.

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Global for Density {}

impl Density {
    /// The one in use (comfortable until set).
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Density>().copied().unwrap_or_default()
    }

    /// `--dcc-composer-padding-x` / `-y`.
    pub fn composer_padding(self) -> (f32, f32) {
        match self {
            Density::Comfortable => (16.0, 12.0),
            Density::Compact => (12.0, 8.0),
        }
    }

    /// `--dcc-composer-input-min-height`.
    pub fn composer_input_min_height(self) -> f32 {
        match self {
            Density::Comfortable => 64.0,
            Density::Compact => 48.0,
        }
    }

    /// `--dcc-terminal-toolbar-height`.
    pub fn terminal_toolbar(self) -> f32 {
        match self {
            Density::Comfortable => 40.0,
            Density::Compact => 34.0,
        }
    }
}
