use bevy::{asset::UntypedHandle, prelude::*};

use super::runtime::effect_pass_gpu_data::EffectPassMaterial;

#[derive(Asset, TypePath, Clone, Debug)]
pub struct MaterialAsset {
    pub netimmerse_texture_binding: Option<NetImmerseMaterialTextureBinding>,
    pub(super) evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    pub(super) _source_dependencies: Box<[UntypedHandle]>,
    pub(super) evaluated_technique_pass_material_assets:
        Box<[EvaluatedTechniquePassMaterialAssets]>,
    pub(super) bound_texture_assets: Box<[Handle<Image>]>,
}

#[derive(Clone, Debug)]
pub(super) struct EvaluatedTechniquePassMaterialAssets {
    pub(super) quality_annotation: Option<f32>,
    pub(super) pass_material_assets: Box<[Handle<EffectPassMaterial>]>,
}

#[derive(Clone, Debug)]
pub struct NetImmerseMaterialTextureBinding {
    pub name: String,
    pub decal_stage: usize,
    pub decal_uv_set: u32,
}
