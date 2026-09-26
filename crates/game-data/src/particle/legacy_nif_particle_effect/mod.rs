//! Particle-effect data from an embedded NIF particle object.

use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyNifParticleEffect {
    pub effect_id: AssetId,
    pub emitter_id: AssetId,
    pub model: String,
    pub source_block: u32,
    pub capacity: u32,
    pub particle_radius: f32,
    pub emit_rate: f32,
    pub emit_start_seconds: f32,
    pub emit_stop_seconds: f32,
    pub lifetime_seconds: [f32; 2],
    pub speed: [f32; 2],
    pub vertical_direction: f32,
    pub vertical_angle: f32,
    pub horizontal_direction: f32,
    pub horizontal_angle: f32,
    pub initial_normal: [f32; 3],
    pub initial_color: [f32; 4],
    pub initial_size: f32,
    pub start_random: [f32; 3],
    pub emitter_block: i32,
    pub mesh_particles: bool,
    pub modifiers: Vec<LegacyNifParticleModifier>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LegacyNifParticleModifier {
    Rotation {
        random_initial_axis: bool,
        initial_axis: [f32; 3],
        radians_per_second: f32,
    },
    ColorOverLifetime {
        interpolation: Option<u32>,
        keys: Vec<LegacyNifParticleColorKey>,
    },
    GrowFade {
        grow_seconds: f32,
        fade_seconds: f32,
    },
    MeshSelection {
        prototype_blocks: Vec<i32>,
    },
    DirectionalGravity {
        direction: [f32; 3],
        acceleration: f32,
    },
    PointGravity {
        position: [f32; 3],
        force: f32,
        decay: f32,
    },
    PlanarCollision {
        bounce: f32,
        spawn_on_collide: bool,
        die_on_collide: bool,
        height: f32,
        width: f32,
        position: [f32; 3],
        x_axis: [f32; 3],
        y_axis: [f32; 3],
        plane_normal: [f32; 3],
        plane_constant: f32,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LegacyNifParticleColorKey {
    pub time: f32,
    pub value: [f32; 4],
    pub forward: Option<[f32; 4]>,
    pub backward: Option<[f32; 4]>,
}
