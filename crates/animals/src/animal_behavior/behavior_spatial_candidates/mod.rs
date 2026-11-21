use bevy::{
    ecs::{entity::EntityLocation, relationship::RelationshipTarget, system::SystemParam},
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::{
    application_schedule::FixedGameSet,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{animal_lifecycle::types::Animal, staff::staff_employment_types::Staff};

const CELL_SIZE_M: f32 = 8.0;

#[derive(Component)]
#[component(storage = "SparseSet")]
#[relationship(relationship_target = BehaviorSpatialMembers)]
struct BehaviorSpatialCell(Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorSpatialCell)]
struct BehaviorSpatialMembers(Vec<Entity>);

#[derive(Resource, Default)]
struct BehaviorSpatialCells(HashMap<IVec2, Entity>);

type SpatialRecipient = Or<(
    With<Animal>,
    With<Staff>,
    (With<WorldMember>, With<DefinitionId>),
)>;
type SpatialFacts = (
    Entity,
    &'static GlobalTransform,
    Option<&'static BehaviorSpatialCell>,
);

pub(super) fn register_behavior_spatial_candidates(app: &mut App) {
    app.init_resource::<BehaviorSpatialCells>()
        .add_systems(
            FixedUpdate,
            project_spatial_candidates
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(OnExit(GamePhase::InGame), retire_spatial_cells);
}

fn project_spatial_candidates(
    moved: Query<
        SpatialFacts,
        (
            SpatialRecipient,
            Or<(
                Changed<GlobalTransform>,
                Added<Animal>,
                Added<Staff>,
                Added<WorldMember>,
                Added<DefinitionId>,
            )>,
        ),
    >,
    all: Query<SpatialFacts, SpatialRecipient>,
    live: Query<(), (With<GlobalTransform>, SpatialRecipient)>,
    mut removed_transforms: RemovedComponents<GlobalTransform>,
    mut removed_animals: RemovedComponents<Animal>,
    mut removed_staff: RemovedComponents<Staff>,
    mut removed_members: RemovedComponents<WorldMember>,
    mut removed_ids: RemovedComponents<DefinitionId>,
    mut cells: ResMut<BehaviorSpatialCells>,
    mut commands: Commands,
) {
    let removed = removed_transforms
        .read()
        .chain(removed_animals.read())
        .chain(removed_staff.read())
        .chain(removed_members.read())
        .chain(removed_ids.read());
    for entity in removed.filter(|&entity| !live.contains(entity)) {
        commands.entity(entity).try_remove::<BehaviorSpatialCell>();
    }
    let initialize = cells.0.is_empty();
    let mut project =
        |(entity, transform, current): (Entity, &GlobalTransform, Option<&BehaviorSpatialCell>)| {
            let point = transform.translation().xz();
            if !point.is_finite() {
                if current.is_some() {
                    commands.entity(entity).remove::<BehaviorSpatialCell>();
                }
                return;
            }
            let coordinate = cell_of(point);
            let cell = match cells.0.get(&coordinate) {
                Some(&cell) => cell,
                None => {
                    let cell = commands.spawn_empty().id();
                    cells.0.insert(coordinate, cell);
                    cell
                }
            };
            if current.is_none_or(|current| current.0 != cell) {
                commands.entity(entity).insert(BehaviorSpatialCell(cell));
            }
        };
    if initialize {
        all.iter().for_each(&mut project);
    } else {
        moved.iter().for_each(&mut project);
    }
}

fn retire_spatial_cells(mut cells: ResMut<BehaviorSpatialCells>, mut commands: Commands) {
    for (_, cell) in cells.0.drain() {
        commands.entity(cell).try_despawn();
    }
}

fn cell_of(point: Vec2) -> IVec2 {
    (point / CELL_SIZE_M).floor().as_ivec2()
}

#[derive(SystemParam)]
pub(super) struct BehaviorSpatialCandidates<'w, 's> {
    cells: Res<'w, BehaviorSpatialCells>,
    members: Query<'w, 's, &'static BehaviorSpatialMembers>,
    locations: Query<'w, 's, EntityLocation>,
}

impl BehaviorSpatialCandidates<'_, '_> {
    pub(super) fn in_bounds(&self, min: Vec2, max: Vec2, candidates: &mut Vec<Entity>) {
        candidates.clear();
        let min = cell_of(min);
        let max = cell_of(max);
        let width = (i64::from(max.x) - i64::from(min.x) + 1).max(0);
        let height = (i64::from(max.y) - i64::from(min.y) + 1).max(0);
        let mut append = |cell: Entity| {
            if let Ok(members) = self.members.get(cell) {
                candidates.extend(members.iter());
            }
        };
        if width.saturating_mul(height) as u64 > self.cells.0.len() as u64 {
            for (&coordinate, &cell) in &self.cells.0 {
                if coordinate.cmpge(min).all() && coordinate.cmple(max).all() {
                    append(cell);
                }
            }
        } else {
            for z in min.y..=max.y {
                for x in min.x..=max.x {
                    if let Some(&cell) = self.cells.0.get(&IVec2::new(x, z)) {
                        append(cell);
                    }
                }
            }
        }
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
