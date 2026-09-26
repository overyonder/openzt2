//! Authored Blue Fang material loading and Bevy D3D9 pass adaptation.

pub(crate) mod authored_material_parameter_lowering;
pub(crate) mod material_asset_types;
pub(crate) mod runtime;

mod authored_material_asset_loading;
mod d3d9_effect_program_evaluation;
mod d3d9_effect_source_asset_loading;
pub(in crate::assets) mod evaluated_effect_material_asset_construction;
mod material_asset_queries;
mod runtime_proof;
mod source;

use bevy::{asset::AssetApp, prelude::*};

use authored_material_asset_loading::MaterialAssetLoader;
use d3d9_effect_source_asset_loading::{
    register_d3d9_effect_source_asset, D3d9EffectSourceAssetLoader,
};
use material_asset_types::MaterialAsset;

pub(crate) struct MaterialAssetPlugin;

impl Plugin for MaterialAssetPlugin {
    fn build(&self, application: &mut App) {
        register_d3d9_effect_source_asset(application);
        runtime::register_effect_pass_material_asset_and_fixed_function_shader(application);
        application
            .init_asset::<MaterialAsset>()
            .preregister_asset_loader::<MaterialAssetLoader>(&["bfmat"])
            .register_asset_loader(D3d9EffectSourceAssetLoader);
        runtime_proof::install_material_runtime_render_world_proof(application);
    }

    fn finish(&self, application: &mut App) {
        let material_asset_loader = MaterialAssetLoader::from_world(application.world_mut());
        application.register_asset_loader(material_asset_loader);
    }
}
