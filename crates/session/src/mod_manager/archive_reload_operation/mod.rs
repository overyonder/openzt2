use std::collections::BTreeMap;

use bevy::{
    asset::{UntypedAssetId, UntypedAssetLoadFailedEvent, UntypedHandle},
    ecs::{
        system::{Deferred, SystemBuffer, SystemMeta},
        world::DeferredWorld,
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
    published: bool,
}

#[derive(Message)]
struct ArchiveReloadAssetNotice {
    operation: Entity,
    asset: UntypedAssetId,
    ready: bool,
}

#[derive(Default)]
struct ArchiveReloadAssetNotices(Vec<ArchiveReloadAssetNotice>);

impl SystemBuffer for ArchiveReloadAssetNotices {
    fn queue(&mut self, _: &SystemMeta, mut world: DeferredWorld<'_>) {
        let _ = world.write_message_batch(self.0.drain(..));
    }
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
                published: false,
            });
    }
}

pub(super) fn register_archive_reload_tracking(app: &mut App) {
    app.add_message::<ArchiveReloadAssetNotice>().add_systems(
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
        openzt2_world::world_spawn::challenge_economy_initialization::ChallengeEconomyInitializer,
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
    operations: Query<(Entity, &AssetArchiveReloadOperation)>,
    mut notices: Deferred<ArchiveReloadAssetNotices>,
) {
    let Some(mut events) = events else {
        return;
    };
    let Ok((operation, reload)) = operations.single() else {
        events.clear();
        return;
    };
    notices.0.extend(events.read().filter_map(|event| {
        let (asset, ready) = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => (id.untyped(), false),
            AssetEvent::LoadedWithDependencies { id } => (id.untyped(), true),
            _ => return None,
        };
        reload
            .pending
            .contains_key(&asset)
            .then_some(ArchiveReloadAssetNotice {
                operation,
                asset,
                ready,
            })
    }));
}

fn finish_archive_reload(
    mut commands: Commands,
    mut operations: Query<(Entity, &mut AssetArchiveReloadOperation)>,
    mut notices: MessageReader<ArchiveReloadAssetNotice>,
    mut failures: MessageReader<UntypedAssetLoadFailedEvent>,
    time: Res<Time<Real>>,
    server: Res<AssetServer>,
    mut menu: MessageWriter<ReturnToMainMenu>,
) {
    let Ok((entity, mut operation)) = operations.single_mut() else {
        notices.clear();
        failures.clear();
        return;
    };
    for notice in notices.read().filter(|notice| notice.operation == entity) {
        let Some(asset) = operation.pending.get_mut(&notice.asset) else {
            continue;
        };
        asset.published |= !notice.ready;
        if !asset.published {
            continue;
        }
        let state = server.recursive_dependency_load_state(notice.asset);
        if state.is_loaded() || state.is_failed() {
            operation.failed |= state.is_failed();
            operation.pending.remove(&notice.asset);
        }
    }
    let mut dependency_failed = false;
    for failure in failures.read() {
        dependency_failed = true;
        if operation.pending.remove(&failure.id).is_some() {
            operation.failed = true;
            warn!(path = %failure.path, error = %failure.error, "archive asset reload failed");
        }
    }
    if dependency_failed {
        let pending_before_failures = operation.pending.len();
        operation.pending.retain(|id, asset| {
            !asset.published || !server.recursive_dependency_load_state(*id).is_failed()
        });
        operation.failed |= operation.pending.len() != pending_before_failures;
    }
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
