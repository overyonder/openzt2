//! Ordered registration of OpenZT2's typed asset loaders and asset resources.

use bevy::{app::PluginGroupBuilder, prelude::*};

pub(crate) struct OpenZt2AssetLoadingPluginGroup;

impl PluginGroup for OpenZt2AssetLoadingPluginGroup {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(super::animation::AnimationAssetLoadingPlugin)
            .add(super::audio::AudioAssetPlugin)
            .add(super::behavior::BehaviorDocumentAssetPlugin)
            .add(super::effect::ParticleEffectAssetAndRuntimePlugin)
            .add(super::localization::LocalizationAssetPlugin)
            .add(super::lua_script::LuaScriptAssetLoadingPlugin)
            .add(super::material::MaterialAssetPlugin)
            .add(super::model::native_model_asset_loading::NativeModelAssetPlugin)
            .add(super::scene_prefab::ScenePrefabAssetPlugin)
            .add(super::species::SpeciesAssetPlugin)
            .add(super::terrain::TerrainAssetPlugin)
            .add(super::world_definitions::WorldDefinitionAssetPlugin)
            .add(super::world_scenario::WorldScenarioDocumentAssetPlugin)
            // UI documents are the only XML family that publishes labels.
            .add(super::ui_document::UiDocumentAssetPlugin)
            // Interactive textures publish image labels, so their loader must
            // own labeled image extensions after Bevy's ordinary image loader.
            .add(super::texture::TextureAssetPlugin)
    }
}
