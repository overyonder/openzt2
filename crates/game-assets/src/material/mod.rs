//! Authored Blue Fang material loading and Bevy D3D9 pass adaptation.

pub mod authored_material_parameter_lowering;
pub mod material_asset_types;
pub mod runtime;

pub mod authored_material_asset_loading;
pub mod d3d9_effect_program_evaluation;
pub mod d3d9_effect_source_asset_loading;
pub mod evaluated_effect_material_asset_construction;
pub mod material_asset_queries;
pub mod runtime_proof;
pub mod source;

use bevy::{asset::AssetApp, prelude::*};

use authored_material_asset_loading::MaterialAssetLoader;
use d3d9_effect_source_asset_loading::{
    register_d3d9_effect_source_asset, D3d9EffectSourceAssetLoader,
};
use material_asset_types::MaterialAsset;

pub struct MaterialAssetPlugin;

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
