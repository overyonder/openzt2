use bevy::prelude::*;
use openzt2_construction::construction::construction_transaction_preparation::SubmittedConstruction;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackContext;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_topology_cell;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;

use super::{
    aquatic_simulation_types::{tank_habitat_seed_cell, Tank, TankGeometry, TankWallTopHeight},
    tank_geometry_edit_types::{
        CommitTankGeometryEdit, TankGeometryEdit, TankGeometryEditAcknowledged,
        TankGeometryEditPreparationRejected, TankGeometryEditPrepared, TankGeometryGesture,
    },
    tank_geometry_rules::{tank_geometry_edit_reaches, tank_geometry_limits},
    tank_height_bounds::TankHeightBoundsQueries,
};

#[derive(Component, Clone, Copy)]
pub(super) struct LiveAppliedTankGeometry(pub(super) Entity);

#[derive(Component)]
pub(super) struct SubmittedTankGeometry {
    edit: TankGeometryEdit,
    live: Option<LiveAppliedTankGeometry>,
}

pub(super) fn capture_submitted_tank_geometry(
    submissions: Query<(Entity, &SubmittedConstruction), Added<SubmittedConstruction>>,
    previews: Query<(
        &TankGeometryEdit,
        &TankGeometryGesture,
        Option<&LiveAppliedTankGeometry>,
    )>,
    mut commands: Commands,
) {
    for (transaction, submission) in submissions {
        let Ok((edit, gesture, live)) = previews.get(submission.source) else {
            continue;
        };
        if !gesture.pending {
            continue;
        }
        commands.entity(transaction).insert(SubmittedTankGeometry {
            edit: edit.clone(),
            live: live.copied(),
        });
        if live.is_some() {
            commands
                .entity(submission.source)
                .remove::<LiveAppliedTankGeometry>();
        }
    }
}

pub(super) fn clear_submitted_tank_geometry(
    submissions: Query<Entity, With<SubmittedTankGeometry>>,
    mut commands: Commands,
) {
    for transaction in submissions {
        commands
            .entity(transaction)
            .remove::<SubmittedTankGeometry>();
    }
}

pub(super) fn restore_unprepared_tank_geometry(
    discarded: On<Discard<SubmittedTankGeometry>>,
    submissions: Query<&SubmittedTankGeometry, Without<TankGeometryEdit>>,
    mut tanks: Query<&mut TankGeometry>,
    boundaries: HabitatBoundaryQueries,
    mut walls: Query<(&PersistentId, &mut TankWallTopHeight)>,
) {
    if let Ok(submission) = submissions.get(discarded.entity) {
        if let Some(live) = submission.live {
            restore_live_tank_geometry(
                &submission.edit,
                live.0,
                &mut tanks,
                &boundaries,
                &mut walls,
            );
        }
    }
}

pub(super) fn restore_uncommitted_tank_geometry(
    discarded: On<Discard<TankGeometryEdit>>,
    edits: Query<(&TankGeometryEdit, &LiveAppliedTankGeometry)>,
    mut tanks: Query<&mut TankGeometry>,
    boundaries: HabitatBoundaryQueries,
    mut walls: Query<(&PersistentId, &mut TankWallTopHeight)>,
) {
    let Ok((edit, live)) = edits.get(discarded.entity) else {
        return;
    };
    restore_live_tank_geometry(edit, live.0, &mut tanks, &boundaries, &mut walls);
}

fn restore_live_tank_geometry(
    edit: &TankGeometryEdit,
    tank: Entity,
    tanks: &mut Query<&mut TankGeometry>,
    boundaries: &HabitatBoundaryQueries,
    walls: &mut Query<(&PersistentId, &mut TankWallTopHeight)>,
) {
    if let Ok(mut geometry) = tanks.get_mut(tank) {
        if *geometry == edit.after {
            geometry.set_if_neq(edit.before);
        }
    }
    for fence in boundaries.fences(tank) {
        let Ok((id, mut current)) = walls.get_mut(fence) else {
            continue;
        };
        if let Ok(index) = edit
            .wall_heights
            .binary_search_by_key(&id.0, |wall| wall.id.0)
        {
            let wall = edit.wall_heights[index];
            if current.0 == wall.after {
                current.set_if_neq(TankWallTopHeight(wall.before));
            }
        }
    }
}

pub(super) fn write_tank_wall_height_images(
    fences: &[Entity],
    edit: &TankGeometryEdit,
    forward: bool,
    walls: &mut Query<(&PersistentId, &mut TankWallTopHeight)>,
) -> bool {
    let Some(updates) = edit
        .wall_heights
        .iter()
        .filter(|wall| wall.before != wall.after)
        .map(|wall| {
            let entity = fences
                .iter()
                .copied()
                .find(|entity| walls.get(*entity).is_ok_and(|(id, _)| *id == wall.id))?;
            let height = if forward { wall.after } else { wall.before };
            Some((entity, TankWallTopHeight(height)))
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    for (entity, height) in updates {
        let (_, mut current) = walls.get_mut(entity).expect("validated tank wall");
        current.set_if_neq(height);
    }
    true
}

/// The live tank an edit addresses: the habitat at its seed cell, still a
/// tank of the edited definition in the same world, still unbreached, and
/// still identified by that seed.
fn resolve_edited_tank(
    edit: &TankGeometryEdit,
    root: Entity,
    habitat_index: &HabitatIndex,
    cell_owners: &Query<&HabitatCellOf>,
    seeds: impl Fn(Entity) -> Option<(Entity, Tank, HabitatRegionFacts)>,
) -> Option<Entity> {
    let habitat = locate_habitat_at_topology_cell(habitat_index, cell_owners, edit.seed_cell)?;
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
    tank: Entity,
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
                let Some(mut walls) = bounds.wall_height_images(tank, limits) else {
                    return false;
                };
                let Some(height_bounds) = bounds.tank_height_bounds(tank, region, limits, &walls)
                else {
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
                tank_geometry_edit_reaches(edit, edit.after, definitions, limits, height_bounds)
            })
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn prepare_tank_geometry_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    transactions: Query<(&SubmittedTankGeometry, &WorldMember)>,
    habitat_indexes: Query<&HabitatIndex>,
    cell_owners: Query<&HabitatCellOf>,
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
        let Ok((submission, world)) = transactions.get(request.transaction) else {
            continue;
        };
        let edit = &submission.edit;
        let live = submission.live;
        let tank = habitat_indexes
            .get(world.root)
            .ok()
            .and_then(|habitat_index| {
                resolve_edited_tank(edit, world.root, habitat_index, &cell_owners, |habitat| {
                    let (tank, geometry, region, summary, member) = tanks.get(habitat).ok()?;
                    let expected = live.map_or(edit.before, |_| edit.after);
                    (*geometry == expected && region.area_square_metres == geometry.area).then(
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
            && live.is_none_or(|live| tank == Some(live.0))
            && tank
                .and_then(|tank| tanks.get(tank).ok().map(|facts| (tank, facts)))
                .is_some_and(|(tank, (_, _, region, ..))| {
                    edit_after_is_reachable(
                        tank,
                        edit,
                        active_definitions.get(&definitions),
                        region,
                        &bounds,
                        live.is_none(),
                        |walls, limits| {
                            bounds.include_adjacent_tank_water_in_wall_minima(
                                walls,
                                tank,
                                limits,
                                |entity| {
                                    let (_, geometry, adjacent, _, adjacent_world) =
                                        tanks.get(entity).ok()?;
                                    (adjacent_world.root == world.root
                                        && tank_habitat_seed_cell(adjacent) != Some(edit.seed_cell))
                                    .then_some(geometry.water_height)
                                },
                            );
                        },
                    )
                });
        if valid {
            commands.entity(request.transaction).insert(edit.clone());
            if let Some(live) = live {
                commands.entity(request.transaction).insert(live);
            }
            prepared.write(TankGeometryEditPrepared {
                transaction: request.transaction,
                cost: Money::ZERO,
            });
        } else {
            rejected.write(TankGeometryEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::InvalidHabitat,
            });
        }
        commands.entity(request.preview).try_despawn();
    }
}

/// Applies the requested image and the affected segments' individual wall tops.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_tank_geometry_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<CommitTankGeometryEdit>,
    transactions: Query<(
        &TankGeometryEdit,
        &WorldMember,
        Option<&RollbackContext>,
        Option<&LiveAppliedTankGeometry>,
    )>,
    habitat_indexes: Query<&HabitatIndex>,
    cell_owners: Query<&HabitatCellOf>,
    mut tanks: Query<(
        &Tank,
        &mut TankGeometry,
        &HabitatRegion,
        &HabitatSummary,
        &WorldMember,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut wall_access: ParamSet<(
        TankHeightBoundsQueries,
        Query<(&PersistentId, &mut TankWallTopHeight)>,
    )>,
    mut acknowledged: MessageWriter<TankGeometryEditAcknowledged>,
) {
    for request in requests.read() {
        let Ok((edit, member, rollback, live)) = transactions.get(request.transaction) else {
            continue;
        };
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
        let already_applied =
            request.application == EditApplication::InitialCommit && live.is_some();
        let (expected, replacement) = if forward {
            (
                if already_applied {
                    edit.after
                } else {
                    edit.before
                },
                edit.after,
            )
        } else {
            (edit.after, edit.before)
        };
        let tank = habitat_indexes
            .get(member.root)
            .ok()
            .and_then(|habitat_index| {
                resolve_edited_tank(edit, member.root, habitat_index, &cell_owners, |habitat| {
                    let (tank, geometry, region, summary, member) = tanks.get(habitat).ok()?;
                    (*geometry == expected && region.area_square_metres == expected.area).then(
                        || {
                            (
                                habitat,
                                *tank,
                                habitat_region_facts(region, summary, member),
                            )
                        },
                    )
                })
            })
            .filter(|tank| !already_applied || live.is_some_and(|live| live.0 == *tank))
            .filter(|tank| {
                let bounds = wall_access.p0();
                tanks.get(*tank).is_ok_and(|(_, _, region, ..)| {
                    edit_after_is_reachable(
                        *tank,
                        edit,
                        active_definitions.get(&definitions),
                        region,
                        &bounds,
                        forward && !already_applied,
                        |walls, limits| {
                            bounds.include_adjacent_tank_water_in_wall_minima(
                                walls,
                                *tank,
                                limits,
                                |entity| {
                                    let (_, geometry, adjacent, _, adjacent_world) =
                                        tanks.get(entity).ok()?;
                                    (adjacent_world.root == member.root
                                        && tank_habitat_seed_cell(adjacent) != Some(edit.seed_cell))
                                    .then_some(geometry.water_height)
                                },
                            );
                        },
                    )
                })
            });
        let accepted = tank.is_some_and(|tank| {
            if already_applied {
                commands
                    .entity(request.transaction)
                    .remove::<LiveAppliedTankGeometry>();
                return true;
            }
            let Ok((_, mut geometry, ..)) = tanks.get_mut(tank) else {
                return false;
            };
            let fences = wall_access.p0().boundary_fences(tank).collect::<Vec<_>>();
            if !write_tank_wall_height_images(&fences, edit, forward, &mut wall_access.p1()) {
                return false;
            }
            geometry.set_if_neq(replacement);
            true
        });
        acknowledged.write(TankGeometryEditAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted,
        });
    }
}
