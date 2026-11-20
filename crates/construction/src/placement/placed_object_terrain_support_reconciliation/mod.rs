//! Reconciles placed-object terrain support after committed terrain changes.

use bevy::{ecs::entity::EntityHashSet, platform::collections::HashMap, prelude::*};
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;
use openzt2_world_state::world_spawn::prefab_source_asset_handle::PrefabSourceAssetHandle;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_cell_indices;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;

use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::rotate_object_placement_cell_by_quarter_turns;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use openzt2_zoo_state::placement::placed_object_types::PhysicsMovedPlacedObjectFootprint;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectTerrainSupportEvaluated;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectTerrainSupportOutcome;

/// Reconciles static authored support links after the sole terrain owner has
/// committed a height edit. The full footprint is validated before the
/// transform is changed, so a failed support update cannot partially move an
/// object or alter occupancy.
pub(super) fn reconcile_static_placed_object_support_after_terrain_changes(
    mut changes: MessageReader<TerrainNavigationChanged>,
    terrain_index: Res<TerrainIndex>,
    placement_index: Res<PlacedObjectFootprintOccupancyIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    footprints: Res<ScenePlacementFootprints>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut placed: Query<
        (
            &PlacedObjectDefinitionReference,
            &PlacedObjectFootprintOccupancy,
            &mut Transform,
            Option<&HabitatMember>,
            Option<&PrefabSourceAssetHandle>,
        ),
        Without<PhysicsMovedPlacedObjectFootprint>,
    >,
    mut evaluated: MessageWriter<PlacedObjectTerrainSupportEvaluated>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let mut affected_objects = EntityHashSet::default();
    for change in changes.read() {
        let Ok((chunk, _)) = chunks.get(change.chunk) else {
            continue;
        };
        let minimum = (chunk.origin
            + change.min.saturating_sub(UVec2::ONE).as_vec2() * chunk.spacing_m)
            .floor()
            .as_ivec2();
        let maximum = (chunk.origin
            + change.max.saturating_add(UVec2::ONE).as_vec2() * chunk.spacing_m)
            .ceil()
            .as_ivec2();
        for z in minimum.y..=maximum.y {
            for x in minimum.x..=maximum.x {
                let cell = IVec2::new(x, z);
                let point = cell.as_vec2();
                if terrain_chunk_at(&terrain_index, point) == Some(change.chunk)
                    && terrain_cell_indices(chunk, point).is_some_and(|sample| {
                        sample.cmple(change.max).all()
                            && sample.saturating_add(UVec2::ONE).cmpge(change.min).all()
                    })
                {
                    affected_objects.extend(placement_index.entities_occupying_cell(cell));
                }
            }
        }
    }
    let mut affected_objects = affected_objects.into_iter().collect::<Vec<_>>();
    affected_objects.sort_unstable();
    let mut terrain_samples = HashMap::new();
    for entity in affected_objects {
        let Ok((placeable, occupancy, mut transform, habitat, prefab)) = placed.get_mut(entity)
        else {
            continue;
        };
        terrain_samples.clear();
        let mut terrain = |position: Vec2| {
            *terrain_samples
                .entry(position.as_ivec2())
                .or_insert_with(|| {
                    let terrain_entity = terrain_chunk_at(&terrain_index, position)?;
                    let (chunk, edited) = chunks.get(terrain_entity).ok()?;
                    let asset = terrain_assets.get(&chunk.asset)?;
                    sample_terrain(chunk, asset, edited, position)
                })
        };
        let Some(definition) = resolve_object_placeable_definition(catalogue, placeable.definition)
        else {
            continue;
        };
        let authored_footprint = footprints
            .resolve(definition, prefab.map(|source| &source.0))
            .cells(occupancy.eighth_turns);
        let occupied_cells = authored_footprint.iter().filter_map(|cell| {
            cell.flags
                .contains_all(FootprintCellFlags::OCCUPIED)
                .then_some({
                    occupancy.origin
                        + rotate_object_placement_cell_by_quarter_turns(
                            IVec2::new(i32::from(cell.offset[0]), i32::from(cell.offset[1])),
                            occupancy.eighth_turns / 2,
                        )
                })
        });
        let Some((support_sum, support_count)) =
            occupied_cells
                .clone()
                .try_fold((0.0_f32, 0.0_f32), |(sum, count), cell| {
                    terrain(cell.as_vec2()).map(|point| (sum + point.height_m, count + 1.0))
                })
        else {
            evaluated.write(PlacedObjectTerrainSupportEvaluated {
                entity,
                outcome: PlacedObjectTerrainSupportOutcome::Rejected(PlacementFailure::OutsideMap),
            });
            continue;
        };
        if support_count <= 0.0 {
            evaluated.write(PlacedObjectTerrainSupportEvaluated {
                entity,
                outcome: PlacedObjectTerrainSupportOutcome::Rejected(PlacementFailure::OutsideMap),
            });
            continue;
        }
        let support_height = support_sum / support_count
            + catalogue
                .find_object(placeable.definition)
                .map_or(0.0, |object| object.terrain_fit_height_offset_m);
        if !support_height.is_finite() {
            evaluated.write(PlacedObjectTerrainSupportEvaluated {
                entity,
                outcome: PlacedObjectTerrainSupportOutcome::Rejected(PlacementFailure::OutsideMap),
            });
            continue;
        }
        let mut candidate = *transform;
        candidate.translation.y = support_height;
        let validity = validate_object_placement_footprint_cells(
            definition,
            authored_footprint,
            &candidate,
            &mut terrain,
            // This reconciliation changes only vertical support. The
            // footprint cells and occupancy index are unchanged from the
            // already-committed placement, so collision cannot change.
            |_| false,
            ObjectPlacementAuthoritativeFacts {
                habitat: habitat.map(|member| member.habitat_entity),
                unlocked: true,
                affordable: true,
                topology_valid: true,
                headroom_valid: true,
                surface_valid: true,
            },
        );
        let outcome = match validity {
            PlacementValidity::Valid { .. }
                if transform.translation.y.to_bits() == support_height.to_bits() =>
            {
                PlacedObjectTerrainSupportOutcome::Stable
            }
            PlacementValidity::Valid { .. } => {
                *transform = candidate;
                PlacedObjectTerrainSupportOutcome::Relocated
            }
            PlacementValidity::Invalid(reason) => {
                PlacedObjectTerrainSupportOutcome::Rejected(reason)
            }
            PlacementValidity::Pending => {
                PlacedObjectTerrainSupportOutcome::Rejected(PlacementFailure::InvalidTopology)
            }
        };
        evaluated.write(PlacedObjectTerrainSupportEvaluated { entity, outcome });
    }
}
