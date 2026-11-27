use bevy::prelude::*;
use openzt2_game_data::world_definitions::fences_and_gates::FenceDefinition;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_gate_prefab;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{WorldDefinitions, WorldDefinitionsView};
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated;
use openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;

use crate::topology::path_support_hydration::PathSupportPrefab;
use crate::topology::topology_presentation_types::{
    EmptyTopologyPresentation, FenceSkewInner, TopologyPresentationRoot,
};
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceSegmentPrefab;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::gate_operation_types::{AuthoredGateVariant, Gate};
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

pub(super) fn authored_gate_definition<'a>(
    definitions: WorldDefinitionsView<'a>,
    fence: AssetId,
    variant: Option<&AuthoredGateVariant>,
) -> Option<&'a FenceDefinition> {
    match variant {
        Some(variant) => definitions.find_fence(variant.0),
        None => definitions.find_gate_fence(fence),
    }
}

#[derive(Component)]
pub(super) struct FenceSegmentPosts {
    post: Handle<ScenePrefabAsset>,
    end_m: f32,
}

pub(super) fn hydrate_topology_prefab_presentations(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Single<&TopologyGrid>,
    mut topology: Query<
        (
            Entity,
            Option<&FenceSegmentPrefab>,
            Option<&FenceTerrainEndpointPresentationCells>,
            Option<&Gate>,
            Option<&AuthoredGateVariant>,
            Option<&FenceEdge>,
            Option<&PathTile>,
            Option<&PathSupportPrefab>,
            Option<&mut Visibility>,
            Option<&Children>,
        ),
        (
            Without<EmptyTopologyPresentation>,
            Or<(
                With<FenceSegmentPrefab>,
                With<PathTile>,
                With<PathSupportPrefab>,
            )>,
        ),
    >,
    presentation_roots: Query<Option<&Children>, With<TopologyPresentationRoot>>,
    inner_skews: Query<(), With<FenceSkewInner>>,
) {
    let grid = grid.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (topology_entity, fence, cells, gate, variant, edge, path, support, visibility, children) in
        &mut topology
    {
        let existing = children
            .into_iter()
            .flat_map(|children| children.iter())
            .find_map(|child| {
                presentation_roots
                    .get(child)
                    .ok()
                    .map(|children| (child, children))
            });
        if let Some((root, root_children)) = existing {
            if fence.is_none()
                || root_children.is_some_and(|children| {
                    children.iter().any(|child| inner_skews.contains(child))
                })
            {
                continue;
            }
            commands.entity(root).try_despawn();
        }
        // Ground-path boundaries own these meshes, including curbs loaded
        // from a saved zoo. A saved edge must not retain its old corner after
        // the neighboring path changes.
        if edge.is_some_and(|edge| definitions.is_automatic_ground_path_curb(edge.definition)) {
            commands
                .entity(topology_entity)
                .insert(EmptyTopologyPresentation);
            continue;
        }
        // A gate edge presents its authored segment model and, beside it, the
        // actor named by its ZTGate physObj binder which carries the leaves.
        let gate_scene = gate.and_then(|_| {
            let fence = authored_gate_definition(definitions, edge?.definition, variant)?;
            select_authored_gate_prefab(fence, variant, |id| {
                definitions
                    .find_fence(id)
                    .map(|variant| variant.gate_policy.prefab)
            })
        });
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
                .insert(EmptyTopologyPresentation);
            continue;
        };
        let Some(handle) = definitions.scene(scene) else {
            continue;
        };
        if let Some(mut visibility) = visibility {
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            commands
                .entity(topology_entity)
                .insert(Visibility::Inherited);
        }
        if fence.is_some() {
            let first_transform = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(topology_entity),
                    TopologyPresentationRoot,
                ))
                .id();
            let second_transform = commands
                .spawn((
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(first_transform),
                    FenceSkewInner,
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
                .and_then(|edge| {
                    if gate.is_some() {
                        authored_gate_definition(definitions, edge.definition, variant)
                    } else {
                        definitions.find_fence(edge.definition)
                    }
                })
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
        } else {
            commands.spawn((
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(topology_entity),
                TopologyPresentationRoot,
                PrefabPresentation::new(handle),
            ));
        }
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
