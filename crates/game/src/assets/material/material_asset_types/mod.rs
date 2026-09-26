use bevy::{asset::UntypedHandle, prelude::*};

use super::runtime::effect_pass_gpu_data::EffectPassMaterial;

#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct MaterialAsset {
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
