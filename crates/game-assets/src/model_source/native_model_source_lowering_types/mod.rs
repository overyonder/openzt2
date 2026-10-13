//! Output records passed from native model lowering to the Bevy asset loader.

pub struct LoweredNativeModelParticleEffect {
    pub labelled_asset_path: String,
    pub particle_effect_document: openzt2_game_data::particle::ParticleEffectDocument,
}

pub struct LoweredNativeModelMaterial {
    pub texture_binding: crate::material::material_asset_types::NetImmerseMaterialTextureBinding,
    pub texture_coordinate_animations: std::sync::Arc<
        [crate::material::runtime::texture_coordinate_animation::TextureCoordinateAnimation],
    >,
    pub labelled_asset_path: String,
    pub evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    pub texture_asset_paths: Vec<(String, String)>,
}

pub struct LoweredNativeModel {
    pub glb: Vec<u8>,
    pub scene_prefab_document: openzt2_game_data::scene_prefab::ScenePrefabDocument,
    pub particle_effects: Vec<LoweredNativeModelParticleEffect>,
    pub materials: Vec<LoweredNativeModelMaterial>,
}
