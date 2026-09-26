//! Reconciles placed-object terrain support after committed terrain changes.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::object_placement::FootprintCellFlags;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::habitat::habitat_types::HabitatMember;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_cell_indices;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_navigation_change_types::TerrainNavigationChanged;

use super::{
    object_placement_definition_queries::{
        resolve_object_placeable_definition, select_authored_footprint_for_eighth_turns,
    },
    object_placement_validation::{
        rotate_object_placement_cell_by_quarter_turns, validate_object_placement_footprint_cells,
        ObjectPlacementAuthoritativeFacts,
    },
    PhysicsMovedPlacedObjectFootprint, PlacedObjectDefinitionReference,
    PlacedObjectFootprintOccupancy, PlacedObjectTerrainSupportEvaluated,
    PlacedObjectTerrainSupportOutcome,
};

/// Reconciles static authored support links after the sole terrain owner has
/// committed a height edit. The full footprint is validated before the
/// transform is changed, so a failed support update cannot partially move an
/// object or alter occupancy.
pub(super) fn reconcile_static_placed_object_support_after_terrain_changes(
    mut changes: MessageReader<TerrainNavigationChanged>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut placed: Query<
        (
            Entity,
            &PlacedObjectDefinitionReference,
            &PlacedObjectFootprintOccupancy,
            &mut Transform,
            Option<&HabitatMember>,
        ),
        Without<PhysicsMovedPlacedObjectFootprint>,
    >,
    mut evaluated: MessageWriter<PlacedObjectTerrainSupportEvaluated>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for change in changes.read() {
        let Ok((changed_chunk, _)) = chunks.get(change.chunk) else {
            continue;
        };
        for (entity, placeable, occupancy, mut transform, habitat) in &mut placed {
            let Some(definition) =
                resolve_object_placeable_definition(catalogue, placeable.definition)
            else {
                continue;
            };
            let authored_footprint =
                select_authored_footprint_for_eighth_turns(definition, occupancy.eighth_turns);
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
            if !occupied_cells.clone().any(|cell| {
                terrain_chunk_at(&terrain_index, cell.as_vec2()) == Some(change.chunk)
                    && terrain_cell_indices(changed_chunk, cell.as_vec2()).is_some_and(|sample| {
                        sample.x <= change.max.x
                            && sample.y <= change.max.y
                            && sample.x.saturating_add(1) >= change.min.x
                            && sample.y.saturating_add(1) >= change.min.y
                    })
            }) {
                continue;
            }

            let terrain = |position: Vec2| {
                let terrain_entity = terrain_chunk_at(&terrain_index, position)?;
                let (chunk, edited) = chunks.get(terrain_entity).ok()?;
                let asset = terrain_assets.get(&chunk.asset)?;
                sample_terrain(chunk, asset, edited, position)
            };
            let Some((support_sum, support_count)) =
                occupied_cells
                    .clone()
                    .try_fold((0.0_f32, 0.0_f32), |(sum, count), cell| {
                        terrain(cell.as_vec2()).map(|point| (sum + point.height_m, count + 1.0))
                    })
            else {
                evaluated.write(PlacedObjectTerrainSupportEvaluated {
                    entity,
                    outcome: PlacedObjectTerrainSupportOutcome::Rejected(
                        PlacementFailure::OutsideMap,
                    ),
                });
                continue;
            };
            if support_count <= 0.0 {
                evaluated.write(PlacedObjectTerrainSupportEvaluated {
                    entity,
                    outcome: PlacedObjectTerrainSupportOutcome::Rejected(
                        PlacementFailure::OutsideMap,
                    ),
                });
                continue;
            }
            let support_height = support_sum / support_count;
            if !support_height.is_finite() {
                evaluated.write(PlacedObjectTerrainSupportEvaluated {
                    entity,
                    outcome: PlacedObjectTerrainSupportOutcome::Rejected(
                        PlacementFailure::OutsideMap,
                    ),
                });
                continue;
            }
            let mut candidate = *transform;
            candidate.translation.y = support_height;
            let validity = validate_object_placement_footprint_cells(
                definition,
                authored_footprint,
                &candidate,
                terrain,
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
}
