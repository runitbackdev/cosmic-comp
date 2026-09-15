// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::input::TouchpadOverride;

pub mod input;
#[cfg(feature = "output")]
pub mod output;
pub mod workspace;

#[derive(Debug, Deserialize, Serialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdidProduct {
    pub manufacturer: [char; 3],
    pub product: u16,
    pub serial: Option<u32>,
    pub manufacture_week: i32,
    pub manufacture_year: i32,
    pub model_year: Option<i32>,
}

#[cfg(feature = "libdisplay-info")]
impl From<libdisplay_info::edid::VendorProduct> for EdidProduct {
    fn from(vp: libdisplay_info::edid::VendorProduct) -> Self {
        Self {
            manufacturer: vp.manufacturer,
            product: vp.product,
            serial: vp.serial,
            manufacture_week: vp.manufacture_week,
            manufacture_year: vp.manufacture_year,
            model_year: vp.model_year,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyboardConfig {
    /// Boot state for numlock
    pub numlock_state: NumlockState,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumlockState {
    BootOn,
    #[default]
    BootOff,
    LastBoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppearanceConfig {
    pub clip_floating_windows: bool,
    pub clip_tiled_windows: bool,
    pub shadow_tiled_windows: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        AppearanceConfig {
            clip_floating_windows: true,
            clip_tiled_windows: true,
            shadow_tiled_windows: false,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecorationPreference {
    #[default]
    ClientSide,
    ServerSide,
}

#[derive(Clone, Debug, PartialEq, CosmicConfigEntry)]
#[version = 1]
pub struct CosmicCompConfig {
    pub workspaces: workspace::WorkspaceConfig,
    pub pinned_workspaces: Vec<workspace::PinnedWorkspace>,
    pub input_default: input::InputConfig,
    pub input_touchpad: input::InputConfig,
    pub input_touchpad_override: TouchpadOverride,
    pub input_devices: HashMap<String, input::InputConfig>,
    pub xkb_config: XkbConfig,
    pub keyboard_config: KeyboardConfig,
    /// Autotiling enabled
    pub autotile: bool,
    /// Determines the behavior of the autotile variable
    /// If set to Global, autotile applies to all windows in all workspaces
    /// If set to PerWorkspace, autotile only applies to new windows, and new workspaces
    pub autotile_behavior: TileBehavior,
    /// Active hint enabled
    pub active_hint: bool,
    /// Enables changing keyboard focus to windows when the cursor passes into them
    pub focus_follows_cursor: bool,
    /// Enables warping the cursor to the focused window when focus changes due to keyboard input
    pub cursor_follows_focus: bool,
    /// The delay in milliseconds before focus follows mouse (if enabled)
    pub focus_follows_cursor_delay: u64,
    /// Let X11 applications scale themselves
    pub descale_xwayland: XwaylandDescaling,
    /// Let X11 applications snoop on certain key-presses to allow for global shortcuts
    pub xwayland_eavesdropping: XwaylandEavesdropping,
    /// The threshold before windows snap themselves to output edges
    pub edge_snap_threshold: u32,
    pub accessibility_zoom: ZoomConfig,
    pub appearance_settings: AppearanceConfig,
    /// Hide the cursor after this many seconds of pointer inactivity (None disables)
    pub cursor_hide_timeout: Option<u32>,
    /// Briefly magnify the cursor when the pointer is shaken, to help locate it
    pub cursor_shake_to_find: bool,
    pub activation_policy: ActivationPolicy,
    pub decoration_preference: DecorationPreference,
    /// Hold the middle button and move to scroll the window under the pointer
    pub middle_click_autoscroll: AutoscrollConfig,
    /// Offer the primary selection (select to copy, middle-click to paste) to clients.
    /// Off, no client is told it exists; clients already connected keep it until they
    /// reconnect.
    pub primary_selection: bool,
    /// The output (by connector name) a seat falls back to when its own has gone or
    /// none has been picked yet, and that becomes the seat's when it appears: where new
    /// windows land after login and after the outputs come back from sleep. Empty, the
    /// first output added, as before.
    pub primary_output: String,
}

impl Default for CosmicCompConfig {
    fn default() -> Self {
        Self {
            workspaces: Default::default(),
            pinned_workspaces: Vec::new(),
            input_default: Default::default(),
            // By default, enable tap-to-click and disable-while-typing.
            input_touchpad: input::InputConfig {
                state: input::DeviceState::Enabled,
                click_method: Some(input::ClickMethod::Clickfinger),
                disable_while_typing: Some(true),
                tap_config: Some(input::TapConfig {
                    enabled: true,
                    button_map: Some(input::TapButtonMap::LeftRightMiddle),
                    drag: true,
                    drag_lock: false,
                }),
                ..Default::default()
            },
            input_touchpad_override: Default::default(),
            input_devices: Default::default(),
            xkb_config: Default::default(),
            keyboard_config: Default::default(),
            autotile: Default::default(),
            autotile_behavior: Default::default(),
            active_hint: true,
            focus_follows_cursor: false,
            cursor_follows_focus: false,
            focus_follows_cursor_delay: 250,
            descale_xwayland: XwaylandDescaling::Fractional,
            xwayland_eavesdropping: XwaylandEavesdropping::default(),
            edge_snap_threshold: 0,
            accessibility_zoom: ZoomConfig::default(),
            appearance_settings: AppearanceConfig::default(),
            cursor_hide_timeout: None,
            cursor_shake_to_find: true,
            activation_policy: ActivationPolicy::default(),
            decoration_preference: DecorationPreference::default(),
            middle_click_autoscroll: AutoscrollConfig::default(),
            primary_selection: true,
            primary_output: String::new(),
        }
    }
}

/// Middle-click autoscroll, the way Windows does it: press the middle button, move past
/// the dead zone and the window under the pointer scrolls at a speed that grows with the
/// distance from the press, until the button is released. A press released inside the
/// dead zone is delivered to the client as an ordinary middle click.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct AutoscrollConfig {
    pub enabled: bool,
    /// Distance from the press, in logical pixels, before scrolling starts
    pub dead_zone: f64,
    /// Speed in logical pixels per millisecond is `multiplier * distance ^ exponent`,
    /// per axis, with the distance measured from the press
    pub multiplier: f64,
    pub exponent: f64,
    /// App ids whose windows keep the middle button for themselves
    pub exclude: Vec<String>,
}

impl Default for AutoscrollConfig {
    fn default() -> Self {
        // Chromium's curve for its own middle-click autoscroll.
        AutoscrollConfig {
            enabled: false,
            dead_zone: 15.0,
            multiplier: 0.000008,
            exponent: 2.2,
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Deserialize, Serialize)]
pub enum TileBehavior {
    #[default]
    Global,
    PerWorkspace,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct XkbConfig {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: Option<String>,
    #[serde(default = "default_repeat_delay")]
    pub repeat_delay: u32,
    #[serde(default = "default_repeat_rate")]
    pub repeat_rate: u32,
}

impl Default for XkbConfig {
    fn default() -> XkbConfig {
        XkbConfig {
            rules: String::new(),
            model: String::new(),
            layout: String::new(),
            variant: String::new(),
            options: None,
            repeat_delay: default_repeat_delay(),
            repeat_rate: default_repeat_rate(),
        }
    }
}

fn default_repeat_rate() -> u32 {
    25
}

fn default_repeat_delay() -> u32 {
    600
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct ZoomConfig {
    pub start_on_login: bool,
    pub show_overlay: bool,
    pub increment: u32,
    pub view_moves: ZoomMovement,
    pub enable_mouse_zoom_shortcuts: bool,
}

impl ZoomConfig {
    pub const ZOOM_INCREMENT_PRESETS: &[u32] = &[10, 25, 50, 75, 100, 150, 200];
}

impl Default for ZoomConfig {
    fn default() -> Self {
        ZoomConfig {
            start_on_login: false,
            show_overlay: true,
            increment: 50,
            view_moves: ZoomMovement::Continuously,
            enable_mouse_zoom_shortcuts: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum ZoomMovement {
    OnEdge,
    Centered,
    Continuously,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct XwaylandEavesdropping {
    pub keyboard: EavesdroppingKeyboardMode,
    pub pointer: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum EavesdroppingKeyboardMode {
    #[default]
    None,
    Modifiers,
    Combinations,
    All,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivationPolicy {
    #[default]
    Focus,
    FocusIfActiveWorkspace,
    Urgent,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum XwaylandDescaling {
    #[serde(rename = "true")]
    Enabled,
    #[serde(rename = "false")]
    Disabled,
    #[default]
    Fractional,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoscroll_config_reads_as_written() {
        let text = "(\n    enabled: true,\n    dead_zone: 15.0,\n    multiplier: 0.000008,\n    exponent: 2.2,\n    exclude: [\"blender\"],\n)\n";
        let config: AutoscrollConfig = ron::from_str(text).unwrap();
        assert!(config.enabled);
        assert_eq!(config.exclude, vec!["blender".to_string()]);
        let default = AutoscrollConfig::default();
        let round_trip: AutoscrollConfig =
            ron::from_str(&ron::to_string(&default).unwrap()).unwrap();
        assert_eq!(round_trip, default);
    }
}
