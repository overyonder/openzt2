use bevy::{platform::collections::HashSet, prelude::*};

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

pub(super) fn index_scene_placement_footprints(
    mut events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    scenes: Res<Assets<ScenePrefabAsset>>,
    mut footprints: ResMut<ScenePlacementFootprints>,
) {
    let changed = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            AssetEvent::LoadedWithDependencies { .. } | AssetEvent::Unused { .. } => None,
        })
        .collect::<HashSet<_>>();
    for id in changed {
        let bounds = scenes.get(id).and_then(|scene| {
            scene
                .canonical_scene_prefab_document()
                .automatic_placement_bounds_xz
        });
        if footprints
            .bypass_change_detection()
            .update_scene_bounds(id, bounds)
        {
            footprints.set_changed();
        }
    }
}
