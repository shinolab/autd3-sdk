use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub(crate) struct Settings {
    pub max_pressure: f32,
    pub show_markers: bool,
    pub sound_speed: f32,
    pub mod_enabled: bool,
    pub playing: bool,
    pub colormap: u8,
    pub bg: [f32; 3],
    pub gizmo_on: bool,
    pub gizmo_rotate: bool,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub move_speed: f32,
    pub slice_resolution: f32,
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
            fov: 60.0,
            near: 1.0,
            far: 5000.0,
            move_speed: 6.0,
            slice_resolution: crate::render::FIELD_TEXELS_PER_MM,
        }
    }
}
