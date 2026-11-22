use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::placement::object_placement_geometry::{
    ScenePlacementFootprintChanged, ScenePlacementFootprints,
};
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

use super::stomped_object_edit_preparation::{
    object_stomp_prevents_placement_at_cell, StompablePlacedObject,
};
use crate::placement::ground_path_placement_stomp::GroundPathPlacementStomp;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::placement::object_placement_definition_queries::{
    collect_occupied_placement_cells_for_transform, resolve_object_placeable_definition,
};
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use openzt2_zoo_state::placement::placed_object_types::IndexedObjectPlacementCells;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;

#[derive(Component)]
pub(super) struct ObjectPlacementPreviewDirty;

#[derive(Component, Clone, PartialEq)]
pub(super) struct ObjectPlacementFit {
    pub transform: Transform,
    pub cost: Money,
    pub cells: Vec<IVec2>,
    pub permission: ObjectPlacementPreviewPermissionFacts,
    pub habitat: Option<Entity>,
}

// Asset reloads and terrain edits must invalidate previews even if the pointer
// and the projected boolean permission facts did not change.
#[allow(clippy::too_many_arguments)]
pub(super) fn mark_changed_object_placement_previews_dirty(
    mut containment: MessageReader<ContainmentChanged>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut scene_changes: MessageReader<ScenePlacementFootprintChanged>,
    placement: PlacedObjectFootprintQueries,
    topology: Query<Ref<TopologyIndex>>,
    terrain_indexes: Query<Ref<TerrainIndex>>,
    previews: Query<(
        Entity,
        Ref<ConstructionPreview>,
        &WorldMember,
        Ref<ObjectPlacementPreviewPermissionFacts>,
        Option<Ref<HabitatMember>>,
        Option<Ref<ObjectPlacementPrefabSource>>,
    )>,
    mut commands: Commands,
) {
    let terrain_changed = terrain_changes.read().count() != 0;
    let scene_changes = scene_changes
        .read()
        .map(|change| change.0)
        .collect::<bevy::platform::collections::HashSet<_>>();
    let authored_inputs_changed = terrain_changed
        || terrain_assets.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed();
    let containment_changed = containment
        .read()
        .map(|change| change.affected_entity)
        .collect::<bevy::platform::collections::HashSet<_>>();
    for (entity, preview, member, permission, habitat, prefab) in &previews {
        if !preview.is_changed()
            && !permission.is_changed()
            && habitat.is_none_or(|habitat| !habitat.is_changed())
            && !containment_changed.contains(&entity)
            && prefab.is_none_or(|prefab| {
                !prefab.is_changed() && !scene_changes.contains(&prefab.0.id())
            })
            && !placement.index_changed(member.root)
            && !topology
                .get(member.root)
                .is_ok_and(|topology| topology.is_changed())
            && !terrain_indexes
                .get(member.root)
                .is_ok_and(|index| index.is_changed())
            && !authored_inputs_changed
        {
            continue;
        }
        commands.entity(entity).insert(ObjectPlacementPreviewDirty);
    }
}

pub(super) fn evaluate_dirty_object_placement_previews(
    mut commands: Commands,
    placement: PlacedObjectFootprintQueries,
    ground_paths: GroundPathPlacementStomp,
    terrain_indexes: Query<&TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    habitats: Query<&HabitatMember>,
    placed: Query<StompablePlacedObject>,
    mut previews: Query<
        (
            Entity,
            &mut ConstructionPreview,
            &WorldMember,
            Option<&ObjectPlacementPreviewPermissionFacts>,
            Option<&PlacedObjectRelocationSource>,
            Option<&ObjectPlacementPrefabSource>,
            Option<&mut ObjectPlacementFit>,
        ),
        (
            With<ObjectPlacementPreviewDirty>,
            Without<IndexedObjectPlacementCells>,
        ),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, mut preview, member, permission, relocation, prefab, existing_fit) in &mut previews
    {
        let Ok(terrain_index) = terrain_indexes.get(member.root) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ObjectPlacementPreviewDirty>();
        let transform = preview.transform;
        let definition_id = preview.definition;
        let Some(permission) = permission else {
            if preview.validity != PlacementValidity::Pending {
                preview.validity = PlacementValidity::Pending;
            }
            continue;
        };
        if !permission.prefab_ready {
            if preview.validity != PlacementValidity::Pending {
                preview.validity = PlacementValidity::Pending;
            }
            continue;
        }
        let Some(definition) = resolve_object_placeable_definition(catalogue, definition_id) else {
            let invalid = PlacementValidity::Invalid(PlacementFailure::AuthoredRule(definition_id));
            if preview.validity != invalid {
                preview.validity = invalid;
            }
            continue;
        };
        let Some(turns) = calculate_authored_object_placement_eighth_turns(definition, &transform)
        else {
            let invalid = PlacementValidity::Invalid(PlacementFailure::AuthoredRule(definition_id));
            if preview.validity != invalid {
                preview.validity = invalid;
            }
            continue;
        };
        let geometry = footprints.resolve(definition, prefab.map(|source| &source.0));
        let terrain = |position: Vec2| {
            let entity = terrain_chunk_at(&terrain_index, position)?;
            let (chunk, edited) = chunks.get(entity).ok()?;
            let asset = terrain_assets.get(&chunk.asset)?;
            sample_terrain(chunk, asset, edited, position)
        };
        let path_overlaps = ground_paths.overlaps(member.root, definition, catalogue);
        let habitat = habitats
            .get(entity)
            .ok()
            .map(|member| member.habitat_entity);
        let validity = validate_object_placement_footprint_cells(
            geometry,
            geometry.cells(turns),
            &transform,
            terrain,
            |cell| {
                path_overlaps.prevents_placement(cell)
                    || object_stomp_prevents_placement_at_cell(
                        cell,
                        definition,
                        catalogue,
                        member.root,
                        &placement,
                        &placed,
                        |entity| Some(entity) == relocation.map(|source| source.0),
                    )
            },
            ObjectPlacementAuthoritativeFacts {
                habitat,
                unlocked: permission.unlocked,
                affordable: permission.affordable,
                topology_valid: permission.topology_valid,
                headroom_valid: permission.headroom_valid,
                surface_valid: permission.surface_valid,
            },
        );
        // A relocation keeps the purchase it was paid with; its preview costs nothing.
        let validity = match validity {
            PlacementValidity::Valid { .. } if relocation.is_some() => {
                PlacementValidity::Valid { cost: Money(0) }
            }
            validity => validity,
        };
        if let PlacementValidity::Valid { cost } = validity {
            let mut cells = Vec::new();
            if collect_occupied_placement_cells_for_transform(geometry, &transform, &mut cells)
                .is_some()
            {
                let fit = ObjectPlacementFit {
                    transform,
                    cost,
                    cells,
                    permission: *permission,
                    habitat,
                };
                if let Some(mut existing_fit) = existing_fit {
                    existing_fit.set_if_neq(fit);
                } else {
                    commands.entity(entity).insert(fit);
                }
            }
        }
        if preview.validity != validity {
            if let PlacementValidity::Invalid(failure) = validity {
                debug!(?failure, definition = ?definition_id, "object placement preview invalid");
            }
            preview.validity = validity;
        }
    }
}
