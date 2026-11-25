//! Output records passed from native model lowering to the Bevy asset loader.

use openzt2_game_data::scene_prefab::{PrefabControllerClock, PrefabScalarCurve};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetImmerseMaterialColorChannel {
    Ambient,
    Diffuse,
    Specular,
    Emissive,
}

#[derive(Debug, Clone)]
pub enum NetImmerseMaterialAnimationTrack {
    Alpha(PrefabScalarCurve),
    Color(NetImmerseMaterialColorChannel, [PrefabScalarCurve; 3]),
}

#[derive(Debug, Clone, Copy)]
pub enum NetImmerseMaterialSample {
    Alpha(f32),
    Color(NetImmerseMaterialColorChannel, [f32; 3]),
}

#[derive(Debug, Clone)]
pub struct NetImmerseMaterialAnimation {
    pub clock: PrefabControllerClock,
    pub track: NetImmerseMaterialAnimationTrack,
}

impl NetImmerseMaterialAnimation {
    pub fn sample(&self, scene_seconds: f32, instance_seconds: f32) -> NetImmerseMaterialSample {
        use crate::scene_prefab::transform_animation_lowering::{controller_time, scalar_at};

        let elapsed = if self.clock.flags & 1 != 0 {
            instance_seconds
        } else {
            scene_seconds
        };
        let time = controller_time(self.clock, elapsed);
        let sample = |curve: &PrefabScalarCurve| scalar_at(curve, time, 0.0).clamp(0.0, 1.0);
        match &self.track {
            NetImmerseMaterialAnimationTrack::Alpha(curve) => {
                NetImmerseMaterialSample::Alpha(scalar_at(curve, time, 1.0))
            }
            NetImmerseMaterialAnimationTrack::Color(channel, [r, g, b]) => {
                NetImmerseMaterialSample::Color(*channel, [sample(r), sample(g), sample(b)])
            }
        }
    }
}

pub struct LoweredNativeModelParticleEffect {
    pub labelled_asset_path: String,
    pub particle_effect_document: openzt2_game_data::particle::ParticleEffectDocument,
}

pub struct LoweredNativeModelMaterial {
    pub texture_binding: crate::material::material_asset_types::NetImmerseMaterialTextureBinding,
    pub texture_coordinate_animations: std::sync::Arc<
        [crate::material::runtime::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
    pub material_animations: std::sync::Arc<[NetImmerseMaterialAnimation]>,
    pub labelled_asset_path: String,
    pub evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    pub texture_asset_paths: Vec<(String, String)>,
    pub reflection_projections: Box<[(usize, usize, [f32; 16])]>,
}

pub struct LoweredNativeModel {
    pub glb: Vec<u8>,
    pub scene_prefab_document: openzt2_game_data::scene_prefab::ScenePrefabDocument,
    pub particle_effects: Vec<LoweredNativeModelParticleEffect>,
    pub materials: Vec<LoweredNativeModelMaterial>,
    pub(crate) embedded_textures: Vec<(String, bevy::prelude::Image)>,
}
