//! Source-faithful NetImmerse particle controllers, modifiers, and particle-data records.

use super::{
    animation_controller_source_types::NetImmerseNiTimeController,
    collision_source_types::NetImmerseNiPlane,
    geometry_data_source_types::NetImmerseNiGeometryData,
};

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleSystemController {
    pub(in super::super) controller: NetImmerseNiTimeController,
    pub(in super::super) speed: f32,
    pub(in super::super) speed_random: f32,
    pub(in super::super) vertical_direction: f32,
    pub(in super::super) vertical_angle: f32,
    pub(in super::super) horizontal_direction: f32,
    pub(in super::super) horizontal_angle: f32,
    pub(in super::super) initial_normal: [f32; 3],
    pub(in super::super) initial_color: [f32; 4],
    pub(in super::super) size: f32,
    pub(in super::super) emit_start_time: f32,
    pub(in super::super) emit_stop_time: f32,
    pub(in super::super) unknown_byte: u8,
    pub(in super::super) emit_rate: f32,
    pub(in super::super) lifetime: f32,
    pub(in super::super) lifetime_random: f32,
    pub(in super::super) emit_flags: u16,
    pub(in super::super) start_random: [f32; 3],
    pub(in super::super) emitter_ref: i32,
    pub(in super::super) unknown_short_2: u16,
    pub(in super::super) unknown_float_13: f32,
    pub(in super::super) unknown_int_1: u32,
    pub(in super::super) unknown_int_2: u32,
    pub(in super::super) unknown_short_3: u16,
    pub(in super::super) num_valid: u16,
    pub(in super::super) particles: Vec<NetImmerseNiParticleInfo>,
    pub(in super::super) unknown_link_ref: i32,
    pub(in super::super) particle_extra_ref: i32,
    pub(in super::super) unknown_link_2_ref: i32,
    pub(in super::super) trailer: u8,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleInfo {
    pub(in super::super) velocity: [f32; 3],
    pub(in super::super) unknown_vector: [f32; 3],
    pub(in super::super) lifetime: f32,
    pub(in super::super) lifespan: f32,
    pub(in super::super) timestamp: f32,
    pub(in super::super) unknown_short: u16,
    pub(in super::super) vertex_id: u16,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleModifier {
    pub(in super::super) next_modifier_ref: i32,
    pub(in super::super) controller_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleRotation {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) random_initial_axis: u8,
    pub(in super::super) initial_axis: [f32; 3],
    pub(in super::super) rotation_speed: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleColorModifier {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) color_data_ref: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleGrowFade {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) grow: f32,
    pub(in super::super) fade: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleMeshModifier {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) particle_mesh_refs: Vec<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiGravity {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) decay: f32,
    pub(in super::super) force: f32,
    pub(in super::super) field_type: u32,
    pub(in super::super) position: [f32; 3],
    pub(in super::super) direction: [f32; 3],
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleCollider {
    pub(in super::super) modifier: NetImmerseNiParticleModifier,
    pub(in super::super) bounce: f32,
    pub(in super::super) spawn_on_collide: bool,
    pub(in super::super) die_on_collide: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiPlanarCollider {
    pub(in super::super) collider: NetImmerseNiParticleCollider,
    pub(in super::super) height: f32,
    pub(in super::super) width: f32,
    pub(in super::super) position: [f32; 3],
    pub(in super::super) x_axis: [f32; 3],
    pub(in super::super) y_axis: [f32; 3],
    pub(in super::super) plane: NetImmerseNiPlane,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticlesData {
    pub(in super::super) geometry: NetImmerseNiGeometryData,
    pub(in super::super) particle_radius: f32,
    pub(in super::super) num_active: u16,
    pub(in super::super) sizes: Option<Vec<f32>>,
    pub(in super::super) rotations: Option<Vec<[f32; 4]>>,
}
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNiParticleMeshesData {
    pub(in super::super) particles: NetImmerseNiParticlesData,
    pub(in super::super) container_node_ref: i32,
}
