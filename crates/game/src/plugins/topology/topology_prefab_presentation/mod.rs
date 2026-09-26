use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabPresentation;

use super::{
    gate_operation_types::GateMechanism,
    path_support_hydration::PathSupportPrefab,
    topology_graph_types::PathTile,
    topology_presentation_types::{
        FenceSegmentPrefab, FenceTerrainEndpointSkewPresentation, TopologyPresentation,
    },
};

pub(super) fn hydrate_topology_prefab_presentations(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    topology: Query<
        (
            Entity,
            Option<&FenceSegmentPrefab>,
            Option<&GateMechanism>,
            Option<&PathTile>,
            Option<&PathSupportPrefab>,
        ),
        (
            Without<TopologyPresentation>,
            Or<(
                With<FenceSegmentPrefab>,
                With<PathTile>,
                With<PathSupportPrefab>,
            )>,
        ),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (topology_entity, fence, gate, path, support) in &topology {
        let scene = gate
            .map(|gate| gate.prefab)
            .filter(|id| *id != AssetId::default())
            .or_else(|| fence.map(|fence| fence.asset))
            .or_else(|| support.map(|support| support.0))
            .or_else(|| {
                let path = path?;
                let path = definitions.find_path(path.definition)?;
                definitions
                    .find_object(path.object)
                    .map(|object| object.prefab)
            });
        let Some(scene) = scene.filter(|id| *id != AssetId::default()) else {
            commands
                .entity(topology_entity)
                .insert(TopologyPresentation(None));
            continue;
        };
        let Some(handle) = definitions.scene(scene) else {
            continue;
        };
        commands
            .entity(topology_entity)
            .insert(Visibility::Inherited);
        let presentation_root = if fence.is_some() {
            let first_transform = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(topology_entity),
                ))
                .id();
            let second_transform = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(first_transform),
                ))
                .id();
            commands.spawn((
                Transform::from_scale(Vec3::new(
                    1.0,
                    1.0,
                    if fence.is_some_and(|fence| fence.mirror_source_y) {
                        -1.0
                    } else {
                        1.0
                    },
                )),
                Visibility::Inherited,
                ChildOf(second_transform),
                PrefabPresentation::new(handle),
            ));
            commands
                .entity(topology_entity)
                .insert(FenceTerrainEndpointSkewPresentation {
                    first_transform,
                    second_transform,
                });
            first_transform
        } else {
            commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(topology_entity),
                    PrefabPresentation::new(handle),
                ))
                .id()
        };
        commands
            .entity(topology_entity)
            .insert(TopologyPresentation(Some(presentation_root)));
    }
}
