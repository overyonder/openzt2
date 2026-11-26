use bevy::{
    ecs::{
        entity::{EntityHashSet, EntityLocation},
        relationship::RelationshipTarget,
        system::SystemParam,
    },
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::{
    application_schedule::FixedGameSet, world_spawn::world_membership_types::WorldMember,
};

use openzt2_world_state::world_spawn::world_membership_types::{WorldMembers, WorldRoot};
use openzt2_zoo_state::locomotion::locomotion_types::{
    NavAgent, SpatialCell, SpatialCellMembers, SpatialGrid,
};

const OVERFLOW_CELL_SIZE_M: f32 = 8.0;

#[derive(Component)]
#[component(storage = "SparseSet")]
#[relationship(relationship_target = BehaviorSpatialMembers)]
struct BehaviorSpatialCell(Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorSpatialCell)]
struct BehaviorSpatialMembers(Vec<Entity>);

#[derive(Component, Default)]
struct BehaviorSpatialOverflow(HashMap<IVec2, Entity>);

type SpatialFacts = (
    Entity,
    &'static GlobalTransform,
    &'static WorldMember,
    Option<&'static BehaviorSpatialCell>,
    Option<&'static SpatialCell>,
    Has<NavAgent>,
);

pub(super) fn register_behavior_spatial_candidates(app: &mut App) {
    app.register_required_components::<WorldRoot, BehaviorSpatialOverflow>()
        .add_observer(remove_missing_spatial_inputs)
        .add_systems(
            FixedUpdate,
            project_spatial_candidates
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame)),
        );
}

fn remove_missing_spatial_inputs(
    removed: On<Remove<(GlobalTransform, WorldMember)>>,
    projected: Query<(), With<BehaviorSpatialCell>>,
    mut commands: Commands,
) {
    if projected.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<BehaviorSpatialCell>();
    }
}

fn project_spatial_candidates(
    moved: Query<
        SpatialFacts,
        Or<(
            Changed<GlobalTransform>,
            Changed<WorldMember>,
            Changed<SpatialCell>,
            Added<NavAgent>,
        )>,
    >,
    all: Query<SpatialFacts>,
    grid_changes: Query<&WorldMembers, Changed<SpatialGrid>>,
    world_members: Query<&WorldMembers>,
    mut removed_cells: RemovedComponents<SpatialCell>,
    mut removed_agents: RemovedComponents<NavAgent>,
    mut removed_grids: RemovedComponents<SpatialGrid>,
    mut worlds: Query<(Option<&SpatialGrid>, &mut BehaviorSpatialOverflow), With<WorldRoot>>,
    mut commands: Commands,
) {
    let mut affected = moved
        .iter()
        .map(|(entity, ..)| entity)
        .collect::<EntityHashSet>();
    affected.extend(removed_cells.read());
    affected.extend(removed_agents.read());
    affected.extend(grid_changes.iter().flat_map(RelationshipTarget::iter));
    for root in removed_grids.read() {
        if let Ok(members) = world_members.get(root) {
            affected.extend(members.iter());
        }
    }
    for (entity, transform, member, current, navigation_cell, navigation_agent) in
        all.iter_many(affected).flatten()
    {
        let Ok((grid, mut overflow)) = worlds.get_mut(member.root) else {
            continue;
        };
        let point = transform.translation().xz();
        let next = if point.is_finite() {
            let in_grid = grid.as_ref().and_then(|grid| {
                grid.cell_of(point)
                    .and_then(|index| grid.cells.get(index as usize).copied())
            });
            if let Some(cell) = in_grid {
                if navigation_agent && navigation_cell.is_some_and(|active| active.0 == cell) {
                    None
                } else {
                    Some(cell)
                }
            } else {
                let coordinate = overflow_cell(point);
                let cell = if let Some(&cell) = overflow.0.get(&coordinate) {
                    cell
                } else {
                    let cell = commands.spawn(ChildOf(member.root)).id();
                    overflow.0.insert(coordinate, cell);
                    cell
                };
                Some(cell)
            }
        } else {
            None
        };
        if current.map(|current| current.0) != next {
            if let Some(cell) = next {
                commands.entity(entity).insert(BehaviorSpatialCell(cell));
            } else {
                commands.entity(entity).remove::<BehaviorSpatialCell>();
            }
        }
    }
}

fn overflow_cell(point: Vec2) -> IVec2 {
    (point / OVERFLOW_CELL_SIZE_M).floor().as_ivec2()
}

#[derive(SystemParam)]
pub(super) struct BehaviorSpatialCandidates<'w, 's> {
    worlds: Query<
        'w,
        's,
        (
            Option<&'static SpatialGrid>,
            &'static BehaviorSpatialOverflow,
        ),
    >,
    members: Query<'w, 's, &'static BehaviorSpatialMembers>,
    navigation_members: Query<'w, 's, &'static SpatialCellMembers>,
    locations: Query<'w, 's, EntityLocation>,
    eligible: Query<'w, 's, &'static WorldMember>,
}

impl BehaviorSpatialCandidates<'_, '_> {
    pub(super) fn in_bounds(
        &self,
        root: Entity,
        min: Vec2,
        max: Vec2,
        candidates: &mut Vec<Entity>,
    ) {
        candidates.clear();
        let Ok((grid, overflow)) = self.worlds.get(root) else {
            return;
        };
        if let Some(grid) = grid.filter(|grid| grid.width != 0 && grid.height != 0) {
            let min = ((min - grid.origin) / grid.cell_size_m)
                .floor()
                .as_ivec2()
                .max(IVec2::ZERO);
            let max = ((max - grid.origin) / grid.cell_size_m)
                .floor()
                .as_ivec2()
                .min(IVec2::new(grid.width as i32 - 1, grid.height as i32 - 1));
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let cell = grid.cells[(y as u32 * grid.width + x as u32) as usize];
                    if let Ok(members) = self.members.get(cell) {
                        candidates.extend(members.iter());
                    }
                    if let Ok(members) = self.navigation_members.get(cell) {
                        candidates.extend(members.iter().filter(|&entity| {
                            self.eligible
                                .get(entity)
                                .is_ok_and(|member| member.root == root)
                        }));
                    }
                }
            }
        }
        let min = overflow_cell(min);
        let max = overflow_cell(max);
        let width = (i64::from(max.x) - i64::from(min.x) + 1).max(0);
        let height = (i64::from(max.y) - i64::from(min.y) + 1).max(0);
        let mut append = |cell: Entity| {
            if let Ok(members) = self.members.get(cell) {
                candidates.extend(members.iter());
            }
        };
        if width.saturating_mul(height) as u64 > overflow.0.len() as u64 {
            for (&coordinate, &cell) in &overflow.0 {
                if coordinate.cmpge(min).all() && coordinate.cmple(max).all() {
                    append(cell);
                }
            }
        } else {
            for z in min.y..=max.y {
                for x in min.x..=max.x {
                    if let Some(&cell) = overflow.0.get(&IVec2::new(x, z)) {
                        append(cell);
                    }
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
    }

    pub(super) fn preserve_query_order(&self, candidates: &mut [Entity]) {
        candidates.sort_unstable_by_key(|&entity| {
            self.locations
                .get(entity)
                .ok()
                .map(|location| (location.table_id.as_u32(), location.table_row.index()))
        });
    }
}
