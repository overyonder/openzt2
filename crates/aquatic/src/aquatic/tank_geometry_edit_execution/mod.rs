use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::DomainAckStatus;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackContext;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;

use super::{
    aquatic_simulation_types::{tank_habitat_seed_cell, Tank, TankGeometry, TankWallTopHeight},
    tank_geometry_edit_types::{
        CommitTankGeometryEdit, TankGeometryEdit, TankGeometryEditAcknowledged,
        TankGeometryEditPreparationRejected, TankGeometryEditPrepared, TankGeometryGesture,
    },
    tank_geometry_rules::{tank_geometry_edit_reaches, tank_geometry_limits},
    tank_height_bounds::TankHeightBoundsQueries,
};

/// The live tank an edit addresses: the habitat at its seed cell, still a
/// tank of the edited definition in the same world, still unbreached, and
/// still identified by that seed.
fn resolve_edited_tank(
    edit: &TankGeometryEdit,
    root: Entity,
    habitat_index: &HabitatIndex,
    seeds: impl Fn(Entity) -> Option<(Entity, Tank, HabitatRegionFacts)>,
) -> Option<Entity> {
    let habitat = habitat_index.habitat_for_topology_cell(edit.seed_cell)?;
    let (entity, tank, facts) = seeds(habitat)?;
    (tank.definition == edit.definition
        && facts.root == root
        && !facts.breached
        && facts.seed_cell == Some(edit.seed_cell))
    .then_some(entity)
}

#[derive(Clone, Copy)]
struct HabitatRegionFacts {
    root: Entity,
    breached: bool,
    seed_cell: Option<IVec2>,
}

fn habitat_region_facts(
    region: &HabitatRegion,
    summary: &HabitatSummary,
    member: &WorldMember,
) -> HabitatRegionFacts {
    HabitatRegionFacts {
        root: member.root,
        breached: summary.boundary_is_breached,
        seed_cell: tank_habitat_seed_cell(region),
    }
}

/// Whether the edit's `after` image obeys the rules and the tank's current
/// surroundings.
fn edit_after_is_reachable(
    edit: &TankGeometryEdit,
    definitions: Option<WorldDefinitionsView<'_>>,
    region: &HabitatRegion,
    bounds: &TankHeightBoundsQueries,
    forward: bool,
    adjacent_water_limits: impl Fn(
        &mut [openzt2_zoo_state::aquatic::tank_geometry_edit_types::TankWallHeightEdit],
        super::tank_geometry_rules::TankGeometryLimits,
    ),
) -> bool {
    definitions.is_some_and(|definitions| {
        definitions.tank_edit_policy().is_some()
            && tank_geometry_limits(definitions).is_some_and(|limits| {
                let Some(mut walls) = bounds.wall_height_images(region, limits) else {
                    return false;
                };
                adjacent_water_limits(&mut walls, limits);
                if walls.len() != edit.wall_heights.len()
                    || !walls
                        .iter()
                        .zip(edit.wall_heights.iter())
                        .all(|(live, saved)| {
                            live.id == saved.id
                                && live.before == (if forward { saved.before } else { saved.after })
                                && (saved.after == saved.before
                                    || (saved.after
                                        == saved.after.max(live.minimum).min(live.maximum)))
                        })
                {
                    return false;
                }
                bounds
                    .tank_height_bounds(region, limits)
                    .is_some_and(|bounds| {
                        tank_geometry_edit_reaches(edit, edit.after, definitions, limits, bounds)
                    })
            })
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn prepare_tank_geometry_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    previews: Query<(&TankGeometryEdit, &TankGeometryGesture, &WorldMember)>,
    transactions: Query<(&EditCommitProgress, &WorldMember)>,
    habitat_index: Res<HabitatIndex>,
    tanks: Query<(
        &Tank,
        &TankGeometry,
        &HabitatRegion,
        &HabitatSummary,
        &WorldMember,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    bounds: TankHeightBoundsQueries,
    (mut prepared, mut rejected): (
        MessageWriter<TankGeometryEditPrepared>,
        MessageWriter<TankGeometryEditPreparationRejected>,
    ),
) {
    for request in requests.read() {
        let Ok((edit, gesture, preview_member)) = previews.get(request.preview) else {
            continue;
        };
        let tank = transactions
            .get(request.transaction)
            .ok()
            .filter(|(progress, member)| {
                progress.phase == EditCommitPhase::Preparing
                    && member.root == preview_member.root
                    && gesture.pending
            })
            .and_then(|(_, member)| {
                resolve_edited_tank(edit, member.root, &habitat_index, |habitat| {
                    let (tank, geometry, region, summary, member) = tanks.get(habitat).ok()?;
                    (*geometry == edit.before && region.area_square_metres == geometry.area).then(
                        || {
                            (
                                habitat,
                                *tank,
                                habitat_region_facts(region, summary, member),
                            )
                        },
                    )
                })
            });
        let valid = edit.before != edit.after
            && tank
                .and_then(|tank| tanks.get(tank).ok())
                .is_some_and(|(_, _, region, ..)| {
                    edit_after_is_reachable(
                        edit,
                        active_definitions.get(&definitions),
                        region,
                        &bounds,
                        true,
                        |walls, limits| {
                            for (_, adjacent_geometry, adjacent_region, _, adjacent_member) in
                                &tanks
                            {
                                if adjacent_member.root == preview_member.root
                                    && tank_habitat_seed_cell(adjacent_region)
                                        != Some(edit.seed_cell)
                                {
                                    bounds.include_adjacent_tank_water_in_wall_minima(
                                        walls,
                                        adjacent_region,
                                        adjacent_geometry.water_height,
                                        limits,
                                    );
                                }
                            }
                        },
                    )
                });
        if valid {
            commands.entity(request.transaction).insert(edit.clone());
            prepared.write(TankGeometryEditPrepared {
                transaction: request.transaction,
                cost: Money(0),
            });
        } else {
            rejected.write(TankGeometryEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::InvalidHabitat,
            });
        }
        commands.entity(request.preview).despawn();
    }
}

/// Applies the requested image and the affected segments' individual wall tops.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_tank_geometry_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<CommitTankGeometryEdit>,
    transactions: Query<(
        &TankGeometryEdit,
        &EditCommitProgress,
        &WorldMember,
        Option<&RollbackContext>,
    )>,
    habitat_index: Res<HabitatIndex>,
    mut tanks: Query<(
        &Tank,
        &mut TankGeometry,
        &HabitatRegion,
        &HabitatSummary,
        &WorldMember,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    bounds: TankHeightBoundsQueries,
    wall_entities: Query<(
        Entity,
        &openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
        &TankWallTopHeight,
    )>,
    mut acknowledged: MessageWriter<TankGeometryEditAcknowledged>,
) {
    for request in requests.read() {
        let Ok((edit, progress, member, rollback)) = transactions.get(request.transaction) else {
            continue;
        };
        if progress.application != request.application {
            continue;
        }
        let forward = match request.application {
            EditApplication::InitialCommit | EditApplication::Redo => true,
            EditApplication::Undo => false,
            EditApplication::FailureRollback => {
                let Some(rollback) = rollback else {
                    continue;
                };
                rollback.failed_application == EditApplication::Undo
            }
        };
        let authorized = if request.application == EditApplication::FailureRollback {
            progress.phase == EditCommitPhase::RollingBack
                && progress.aquatic
                    == if forward {
                        DomainAckStatus::Reverted
                    } else {
                        DomainAckStatus::Applied
                    }
        } else {
            progress.phase == EditCommitPhase::Applying
                && progress.aquatic == DomainAckStatus::Pending
        };
        if !authorized {
            continue;
        }
        let (expected, replacement) = if forward {
            (edit.before, edit.after)
        } else {
            (edit.after, edit.before)
        };
        let tank = resolve_edited_tank(edit, member.root, &habitat_index, |habitat| {
            let (tank, geometry, region, summary, member) = tanks.get(habitat).ok()?;
            (*geometry == expected && region.area_square_metres == expected.area).then(|| {
                (
                    habitat,
                    *tank,
                    habitat_region_facts(region, summary, member),
                )
            })
        })
        .filter(|tank| {
            tanks.get(*tank).is_ok_and(|(_, _, region, ..)| {
                edit_after_is_reachable(
                    edit,
                    active_definitions.get(&definitions),
                    region,
                    &bounds,
                    forward,
                    |walls, limits| {
                        for (_, adjacent_geometry, adjacent_region, _, adjacent_member) in &tanks {
                            if adjacent_member.root == member.root
                                && tank_habitat_seed_cell(adjacent_region) != Some(edit.seed_cell)
                            {
                                bounds.include_adjacent_tank_water_in_wall_minima(
                                    walls,
                                    adjacent_region,
                                    adjacent_geometry.water_height,
                                    limits,
                                );
                            }
                        }
                    },
                )
            })
        });
        let accepted = tank.is_some_and(|tank| {
            let Ok((_, mut geometry, region, ..)) = tanks.get_mut(tank) else {
                return false;
            };
            *geometry = replacement;
            for wall in edit.wall_heights.iter() {
                if wall.before == wall.after {
                    continue;
                }
                let height = if forward { wall.after } else { wall.before };
                for (entity, id, _) in &wall_entities {
                    if *id == wall.id && region.boundary_fence_entities.contains(&entity) {
                        commands.entity(entity).insert(TankWallTopHeight(height));
                    }
                }
            }
            true
        });
        acknowledged.write(TankGeometryEditAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted,
        });
    }
}
