use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use crate::plugins::world_spawn::prefab_presentation_types::{
    PrefabPresentation, PrefabPresentationHydrated,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use crate::plugins::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;

use super::{
    gate_operation_types::Gate,
    path_support_hydration::PathSupportPrefab,
    topology_graph_types::{FenceEdge, PathTile, TopologyGrid},
    topology_presentation_types::{
        FenceSegmentPrefab, FenceTerrainEndpointPresentationCells,
        FenceTerrainEndpointSkewPresentation, TopologyPresentation,
    },
};

/// Authored `start_post`/`end_post` binder waiting for its segment model.
/// `end_m` places the end post when the model has no `end_post` node.
#[derive(Component)]
pub(super) struct FenceSegmentPosts {
    post: Handle<ScenePrefabAsset>,
    end_m: f32,
}

pub(super) fn hydrate_topology_prefab_presentations(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    topology: Query<
        (
            Entity,
            Option<&FenceSegmentPrefab>,
            Option<&FenceTerrainEndpointPresentationCells>,
            Option<&Gate>,
            Option<&FenceEdge>,
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
    for (topology_entity, fence, cells, gate, edge, path, support) in &topology {
        // A gate edge presents its authored segment model and, beside it, the
        // actor named by its ZTGate physObj binder which carries the leaves.
        let gate_scene = gate
            .and_then(|_| definitions.find_gate_fence(edge?.definition))
            .map(|fence| fence.gate_policy.prefab)
            .filter(|id| *id != AssetId::default());
        let gate_handle = match gate_scene {
            Some(scene) => match definitions.scene(scene) {
                Some(handle) => Some(handle),
                None => continue,
            },
            None => None,
        };
        let scene = fence
            .map(|fence| fence.asset)
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
            let mut model = commands.spawn((
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
            let post = edge
                .and_then(|edge| definitions.find_fence(edge.definition))
                .map(|fence| fence.post_prefab)
                .filter(|id| *id != AssetId::default())
                .and_then(|id| definitions.scene(id));
            if let (Some(post), Some(cells)) = (post, cells) {
                let [first, second] = cells.endpoint_cells();
                model.insert(FenceSegmentPosts {
                    post,
                    end_m: grid
                        .cell_translation(first)
                        .xz()
                        .distance(grid.cell_translation(second).xz()),
                });
            }
            if let Some(gate_handle) = gate_handle {
                // The gate actor is authored around its centre, between the
                // hinges; segment models start at their first post.
                let half_m = cells.map_or(0.0, |cells| {
                    let [first, second] = cells.endpoint_cells();
                    grid.cell_translation(first)
                        .xz()
                        .distance(grid.cell_translation(second).xz())
                        * 0.5
                });
                commands.spawn((
                    Transform::from_xyz(half_m, 0.0, 0.0),
                    Visibility::Inherited,
                    ChildOf(second_transform),
                    PrefabPresentation::new(gate_handle),
                    AnimationPresentationOwner {
                        gameplay_entity: topology_entity,
                    },
                ));
            }
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

/// Places the post binders once the segment model exists: the start post at
/// the segment origin, the end post on the model's `end_post` node (authored
/// only by curves, which stop short of the corner) or at the segment's end.
pub(super) fn attach_fence_posts_to_hydrated_segment_models(
    mut commands: Commands,
    models: Query<(Entity, &FenceSegmentPosts, &ChildOf), Added<PrefabPresentationHydrated>>,
    children: Query<&Children>,
    identifiers: Query<&PrefabAuthoredAttachmentIdentifier>,
) {
    let end_post = AssetId::from_key("end_post");
    for (model, posts, parent) in &models {
        let end_node = children
            .iter_descendants::<Children>(model)
            .find(|entity| identifiers.get(*entity).is_ok_and(|id| id.0 == end_post));
        let (end_parent, end_transform) = end_node.map_or_else(
            || (parent.parent(), Transform::from_xyz(posts.end_m, 0.0, 0.0)),
            |node| (node, Transform::IDENTITY),
        );
        for (post_parent, transform) in [
            (parent.parent(), Transform::IDENTITY),
            (end_parent, end_transform),
        ] {
            commands.spawn((
                transform,
                Visibility::Inherited,
                ChildOf(post_parent),
                PrefabPresentation::new(posts.post.clone()),
            ));
        }
        commands.entity(model).remove::<FenceSegmentPosts>();
    }
}
