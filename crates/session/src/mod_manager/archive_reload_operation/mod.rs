use std::collections::BTreeMap;

use bevy::{
    asset::{
        LoadState, RecursiveDependencyLoadState, UntypedAssetId, UntypedAssetLoadFailedEvent,
        UntypedHandle,
    },
    prelude::*,
};
use openzt2_game_assets::{
    animation::animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
    audio::audio_asset_types::AudioAsset,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    effect::ParticleEffectDocumentAsset,
    graphics_settings_presets::GraphicsSettingsPresets,
    localization::localization_asset_types::LocalizationAsset,
    lua_script::lua_script_asset_loading::LuaScriptAsset,
    material::material_asset_types::MaterialAsset,
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::SpeciesAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_definitions::{
        ground_path_surface_images::GroundPathSurfaceImage,
        path_placement_highlight_asset::PathPlacementHighlightAsset,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

use crate::shell::shell_navigation_request_types::ReturnToMainMenu;

use super::mod_manager_ui_presentation::{
    synchronize_mod_manager_visibility_and_reload_input_blocking,
    toggle_selected_asset_archive_enabled_state,
};

#[derive(Component)]
pub(super) struct AssetArchiveReloadOperation {
    pending: BTreeMap<UntypedAssetId, ExpectedAssetReload>,
    stalled: Timer,
    failed: bool,
}

struct ExpectedAssetReload {
    _handle: UntypedHandle,
    // The previous value can remain loaded until the source watcher starts the new load.
    observed_replacement: bool,
    observed_loading: bool,
}

impl AssetArchiveReloadOperation {
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            stalled: Timer::from_seconds(30.0, TimerMode::Once),
            failed: false,
        }
    }

    pub(super) fn expect_reload(&mut self, handle: UntypedHandle) {
        self.pending
            .entry(handle.id())
            .or_insert(ExpectedAssetReload {
                _handle: handle,
                observed_replacement: false,
                observed_loading: false,
            });
    }
}

pub(super) fn register_archive_reload_tracking(app: &mut App) {
    app.add_systems(
        Update,
        finish_archive_reload
            .after(openzt2_interface_state::ui::UiSet::Interaction)
            .before(synchronize_mod_manager_visibility_and_reload_input_blocking)
            .before(toggle_selected_asset_archive_enabled_state),
    );
    macro_rules! track {
        ($($asset:ty),* $(,)?) => {
            $(app.add_systems(Update, observe_reloaded_assets::<$asset>.before(finish_archive_reload));)*
        };
    }
    track!(
        UiDocumentAsset,
        Image,
        bevy::audio::AudioSource,
        bevy::gltf::Gltf,
        Font,
        bevy::shader::Shader,
        AnimationClipSourceAsset,
        AnimationSetAsset,
        AudioAsset,
        BehaviorDocumentAsset,
        GraphicsSettingsPresets,
        GroundPathSurfaceImage,
        InteractiveTextureMetadataAsset,
        LocalizationAsset,
        LuaScriptAsset,
        MaterialAsset,
        ParticleEffectDocumentAsset,
        PathPlacementHighlightAsset,
        ScenePrefabAsset,
        SpeciesAsset,
        TerrainAsset,
        WorldDefinitionAsset,
        WorldScenarioDocumentAsset,
    );
}

fn observe_reloaded_assets<A: Asset>(
    events: Option<MessageReader<AssetEvent<A>>>,
    mut operations: Query<&mut AssetArchiveReloadOperation>,
) {
    let Some(mut events) = events else {
        return;
    };
    let Ok(mut operation) = operations.single_mut() else {
        events.clear();
        return;
    };
    for event in events.read() {
        let id = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => id.untyped(),
            _ => continue,
        };
        if let Some(expected) = operation.pending.get_mut(&id) {
            expected.observed_replacement = true;
        }
    }
}

fn finish_archive_reload(
    mut commands: Commands,
    mut operations: Query<(Entity, &mut AssetArchiveReloadOperation)>,
    mut failures: MessageReader<UntypedAssetLoadFailedEvent>,
    time: Res<Time<Real>>,
    server: Res<AssetServer>,
    mut menu: MessageWriter<ReturnToMainMenu>,
) {
    let Ok((entity, mut operation)) = operations.single_mut() else {
        failures.clear();
        return;
    };
    for failure in failures.read() {
        if operation.pending.remove(&failure.id).is_some() {
            operation.failed = true;
            warn!(path = %failure.path, error = %failure.error, "archive asset reload failed");
        }
    }
    let mut dependency_failed = false;
    operation.pending.retain(|id, expected| {
        let Some((asset, _, dependencies)) = server.get_load_states(*id) else {
            return true;
        };
        expected.observed_loading |= asset.is_loading();
        if !expected.observed_replacement && !expected.observed_loading {
            return true;
        }
        match (asset, dependencies) {
            (LoadState::Loaded, RecursiveDependencyLoadState::Loaded) => false,
            (LoadState::Failed(_), _) | (_, RecursiveDependencyLoadState::Failed(_)) => {
                dependency_failed = true;
                false
            }
            _ => true,
        }
    });
    operation.failed |= dependency_failed;
    if !operation.pending.is_empty() && !operation.stalled.tick(time.delta()).is_finished() {
        return;
    }
    if operation.failed || !operation.pending.is_empty() {
        warn!(
            remaining = operation.pending.len(),
            "archive reload ended with unavailable assets"
        );
    }
    commands.entity(entity).despawn();
    menu.write(ReturnToMainMenu);
}
