//! Output records passed from native model lowering to the Bevy asset loader.

pub(in crate::assets) struct LoweredNativeModelParticleEffect {
    pub(in crate::assets) labelled_asset_path: String,
    pub(in crate::assets) particle_effect_document:
        openzt2_game_data::particle::ParticleEffectDocument,
}

pub(in crate::assets) struct LoweredNativeModelMaterial {
    pub(in crate::assets) texture_coordinate_animations: std::sync::Arc<[crate::assets::material::runtime::texture_coordinate_animation::TextureCoordinateAnimation]>,
    pub(in crate::assets) labelled_asset_path: String,
    pub(in crate::assets) evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    pub(in crate::assets) texture_asset_paths: Vec<(String, String)>,
}

pub(in crate::assets) struct LoweredNativeModel {
    pub(in crate::assets) glb: Vec<u8>,
    pub(in crate::assets) scene_prefab_document:
        openzt2_game_data::scene_prefab::ScenePrefabDocument,
    pub(in crate::assets) particle_effects: Vec<LoweredNativeModelParticleEffect>,
    pub(in crate::assets) materials: Vec<LoweredNativeModelMaterial>,
}
