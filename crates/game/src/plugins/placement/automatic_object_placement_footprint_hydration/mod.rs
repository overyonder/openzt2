//! Hydrates authored automatic footprints from scene-prefab bounds.

use bevy::{platform::collections::HashSet, prelude::*};

use crate::{
    assets::{
        scene_prefab::ScenePrefabAsset,
        world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
    plugins::{
        construction::construction_interaction_types::ConstructionPreview,
        world_spawn::{
            prefab_source_asset_handle::PrefabSourceAssetHandle,
            world_membership_types::DefinitionId,
        },
    },
};

use super::ObjectPlacementPrefabSource;

pub(super) fn hydrate_automatic_object_placement_footprints_from_loaded_scene_prefabs(
    mut definitions: ResMut<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    previews: Query<(Ref<ConstructionPreview>, Ref<ObjectPlacementPrefabSource>)>,
    placed_objects: Query<(Ref<DefinitionId>, Ref<PrefabSourceAssetHandle>)>,
) {
    let definitions_changed = definitions.is_changed() || active_definitions.is_changed();
    let changed_prefabs = prefab_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    for (definition, prefab_handle) in previews
        .iter()
        .filter(|(preview, prefab)| {
            definitions_changed
                || changed_prefabs.contains(&prefab.0.id())
                || preview.is_changed()
                || prefab.is_changed()
        })
        .map(|(preview, prefab)| (preview.definition, prefab.0.clone()))
        .chain(
            placed_objects
                .iter()
                .filter(|(definition, prefab)| {
                    definitions_changed
                        || changed_prefabs.contains(&prefab.0.id())
                        || definition.is_changed()
                        || prefab.is_changed()
                })
                .map(|(definition, prefab)| (definition.0, prefab.0.clone())),
        )
    {
        let Some(bounds_xz) = prefabs.get(&prefab_handle).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .automatic_placement_bounds_xz
        }) else {
            continue;
        };
        active_definitions.apply_source_lowered_automatic_placement_bounds(
            &mut definitions,
            definition,
            bounds_xz,
        );
    }
}
