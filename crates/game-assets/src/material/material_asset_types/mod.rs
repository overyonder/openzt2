use bevy::{
    asset::{UntypedAssetId, UntypedHandle, VisitAssetDependencies},
    prelude::*,
};

use super::runtime::effect_pass_gpu_data::EffectPassMaterial;

#[derive(TypePath, Clone, Debug)]
pub struct MaterialAsset {
    pub netimmerse_texture_binding: Option<NetImmerseMaterialTextureBinding>,
    pub(super) evaluated_d3d9_effect: d3d9_effects::effect_types::EvaluatedD3d9Effect,
    pub(super) _source_dependencies: Box<[UntypedHandle]>,
    pub(super) evaluated_technique_pass_material_assets:
        Box<[EvaluatedTechniquePassMaterialAssets]>,
    pub(super) bound_texture_assets: Box<[MaterialTextureAssetBinding]>,
    pub(super) modes: Box<[MaterialModeAsset]>,
}

#[derive(Clone, Debug)]
pub(super) struct MaterialTextureAssetBinding {
    pub(super) parameter_name: String,
    pub(super) texture: Handle<Image>,
}

impl Asset for MaterialAsset {}

impl VisitAssetDependencies for MaterialAsset {
    fn visit_dependencies(&self, visit: &mut impl FnMut(UntypedAssetId)) {
        for mode in &self.modes {
            mode.visit_dependencies(visit);
        }
    }
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub(super) struct MaterialModeAsset {
    pub(super) name: String,
    #[dependency]
    pub(super) target: MaterialModeTarget,
}

#[derive(Clone, Debug, VisitAssetDependencies)]
pub enum MaterialModeTarget {
    ThisMaterial,
    Material(#[dependency] Handle<MaterialAsset>),
}

impl MaterialAsset {
    pub fn mode(&self, name: &str) -> Option<&MaterialModeTarget> {
        self.modes
            .iter()
            .rev()
            .find(|mode| mode.name.eq_ignore_ascii_case(name))
            .map(|mode| &mode.target)
    }
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
