use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    locomotion::locomotion_types::{SpatialCell, SpatialGrid, SpatialGridMembership},
    transport_tours::{
        transport_topology_types::{TransportStation, TransportTrackJunction},
        transport_track_construction_types::TransportTrackConstructionPiece,
    },
};

type PickableTransport = Or<(
    With<TransportStation>,
    With<TransportTrackJunction>,
    With<TransportTrackConstructionPiece>,
)>;

pub(super) fn project_transport_spatial_membership(
    mut commands: Commands,
    grids: Query<&SpatialGrid>,
    changed_grids: Query<(), Changed<SpatialGrid>>,
    candidates: Query<
        (Entity, &GlobalTransform, &WorldMember, Option<&SpatialCell>),
        PickableTransport,
    >,
    changes: Query<
        Entity,
        (
            PickableTransport,
            Or<(
                Changed<GlobalTransform>,
                Changed<WorldMember>,
                Added<TransportStation>,
                Added<TransportTrackJunction>,
                Added<TransportTrackConstructionPiece>,
            )>,
        ),
    >,
) {
    let mut project = |(entity, pose, world, current): (
        Entity,
        &GlobalTransform,
        &WorldMember,
        Option<&SpatialCell>,
    )| {
        let Ok(grid) = grids.get(world.root) else {
            return;
        };
        let next = grid
            .cell_of(pose.translation().xz())
            .and_then(|index| grid.cells.get(index as usize))
            .copied();
        if current.map(|current| current.0) == next {
            return;
        }
        if let Some(cell) = next {
            commands.entity(entity).insert(SpatialCell(cell));
        } else {
            commands.entity(entity).remove::<SpatialCell>();
        }
    };
    if changed_grids.is_empty() {
        candidates
            .iter_many(changes.iter())
            .filter_map(Result::ok)
            .for_each(&mut project);
    } else {
        let changed: EntityHashSet = changes.iter().collect();
        candidates
            .iter()
            .filter(|(entity, _, world, _)| {
                changed.contains(entity) || changed_grids.contains(world.root)
            })
            .for_each(project);
    }
}

#[derive(SystemParam)]
pub struct TransportSpatialPicking<'w, 's> {
    grids: Query<'w, 's, &'static SpatialGrid>,
    membership: SpatialGridMembership<'w, 's>,
}

impl TransportSpatialPicking<'_, '_> {
    fn extend_near(&self, root: Entity, point: Vec2, radius: f32, candidates: &mut EntityHashSet) {
        let Ok(grid) = self.grids.get(root) else {
            return;
        };
        if grid.width == 0 || grid.height == 0 || grid.cell_size_m <= 0.0 || !point.is_finite() {
            return;
        }
        let relative = (point - grid.origin) / grid.cell_size_m;
        let padding = Vec2::splat(radius / grid.cell_size_m);
        let low = (relative - padding).floor().as_ivec2().max(IVec2::ZERO);
        let high = (relative + padding)
            .floor()
            .as_ivec2()
            .min(IVec2::new(grid.width as i32 - 1, grid.height as i32 - 1));
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                candidates.extend(
                    self.membership
                        .members(root, y as u32 * grid.width + x as u32),
                );
            }
        }
    }

    pub fn nearby(&self, root: Entity, point: Vec2, radius: f32) -> EntityHashSet {
        let mut candidates = EntityHashSet::default();
        self.extend_near(root, point, radius, &mut candidates);
        candidates
    }

    pub fn extend_along_ray(
        &self,
        root: Entity,
        ray: Ray3d,
        radius: f32,
        candidates: &mut EntityHashSet,
    ) {
        let Ok(grid) = self.grids.get(root) else {
            return;
        };
        let origin = ray.origin.xz();
        let direction = ray.direction.xz();
        if direction.length_squared() <= f32::EPSILON {
            self.extend_near(root, origin, radius, candidates);
            return;
        }
        let size = Vec2::new(grid.width as f32, grid.height as f32) * grid.cell_size_m;
        let min = grid.origin - Vec2::splat(radius);
        let max = grid.origin + size + Vec2::splat(radius);
        let mut enter = 0.0f32;
        let mut exit = f32::INFINITY;
        for axis in 0..2 {
            if direction[axis].abs() <= f32::EPSILON {
                if !(min[axis]..=max[axis]).contains(&origin[axis]) {
                    return;
                }
            } else {
                let a = (min[axis] - origin[axis]) / direction[axis];
                let b = (max[axis] - origin[axis]) / direction[axis];
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
            }
        }
        if exit < enter || grid.cell_size_m <= 0.0 {
            return;
        }
        let interval = grid.cell_size_m * 0.5 / direction.length();
        let count = ((exit - enter) / interval).ceil() as u32;
        for index in 0..=count {
            let distance = (enter + index as f32 * interval).min(exit);
            self.extend_near(root, origin + direction * distance, radius, candidates);
        }
    }
}
