use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "autd3-simulator-settings";

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct Settings {
    pub max_pressure: f32,
    pub show_markers: bool,
    pub sound_speed: f32,
    pub mod_enabled: bool,
    pub playing: bool,
    pub colormap: u8,
    pub bg: [f32; 3],
    pub gizmo_on: bool,
    pub gizmo_rotate: bool,
    pub cam_free: bool,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub move_speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_pressure: 8000.0,
            show_markers: true,
            sound_speed: 340_000.0,
            mod_enabled: false,
            playing: true,
            colormap: 0,
            bg: crate::render::DEFAULT_BG_RGB,
            gizmo_on: true,
            gizmo_rotate: false,
            cam_free: false,
            fov: 60.0,
            near: 1.0,
            far: 5000.0,
            move_speed: 6.0,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

impl Settings {
    #[must_use]
    pub fn load() -> Self {
        storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(s) = storage()
            && let Ok(json) = serde_json::to_string(self)
        {
            let _ = s.set_item(STORAGE_KEY, &json);
        }
    }
}

#[must_use]
pub fn rgb_to_hex(rgb: [f32; 3]) -> String {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

#[must_use]
pub fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return None;
    }
    let component = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| f32::from(v) / 255.0)
    };
    Some([component(0)?, component(2)?, component(4)?])
}
