pub(super) mod projection;

use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;

use super::guest_influence_watcher::{GuestInfluenceCells, GuestInfluenceEntry};

#[derive(Clone, Copy, PartialEq)]
pub enum InfluenceRadiusDropoff {
    Linear,
    Logarithmic,
    Quadratic,
    Constant,
}

#[derive(Clone, Copy, PartialEq)]
pub struct GuestInfluenceEmission {
    pub value: GuestInfluenceEntry,
    pub radius: f32,
    pub dropoff: InfluenceRadiusDropoff,
}

#[derive(Component, PartialEq)]
pub struct GuestInfluenceEmitter(pub Vec<GuestInfluenceEmission>);

#[derive(Component, Clone, Copy, PartialEq)]
pub struct GuestInfluenceUpdatePolicy {
    pub delay_seconds: f32,
    pub cells_per_update: u32,
}

#[derive(Component, Default, PartialEq)]
pub(super) struct GuestTerrainInfluence(Vec<GuestInfluenceEntry>);

impl GuestTerrainInfluence {
    pub(super) fn entries(&self) -> &[GuestInfluenceEntry] {
        &self.0
    }
}

#[derive(Message)]
pub struct DirtyGuestInfluenceRegion {
    pub root: Entity,
    pub bounds: IRect,
}

#[derive(Message)]
pub struct UpdateGuestInfluenceCells {
    pub root: Entity,
    pub delta_seconds: f32,
}

#[derive(Component, Default)]
pub(super) struct GuestInfluenceRegionWork {
    pending: Option<IRect>,
    delay: f32,
    active: Option<ActiveInfluenceRegion>,
}

struct ActiveInfluenceRegion {
    bounds: IRect,
    next: IVec2,
    emitters: Vec<(Vec2, GuestInfluenceEmission)>,
}

fn emission_at(emission: GuestInfluenceEmission, distance: f32) -> Option<GuestInfluenceEntry> {
    if emission.radius <= 0.0 || distance >= emission.radius {
        return None;
    }
    let remaining = (emission.radius - distance) / emission.radius;
    let factor = match emission.dropoff {
        InfluenceRadiusDropoff::Linear => remaining,
        InfluenceRadiusDropoff::Logarithmic => (1.0 + remaining.log10()).max(0.0),
        InfluenceRadiusDropoff::Quadratic => remaining * remaining,
        InfluenceRadiusDropoff::Constant => 1.0,
    };
    Some(GuestInfluenceEntry {
        intensity: emission.value.intensity * factor,
        ..emission.value
    })
}

pub(super) fn invalidate_guest_influence_regions(
    mut dirty: MessageReader<DirtyGuestInfluenceRegion>,
    mut roots: Query<(
        &SpatialGrid,
        &GuestInfluenceUpdatePolicy,
        &mut GuestInfluenceRegionWork,
    )>,
) {
    for dirty in dirty.read() {
        let Ok((grid, policy, mut work)) = roots.get_mut(dirty.root) else {
            continue;
        };
        let min = dirty.bounds.min.max(IVec2::ZERO);
        let max = dirty
            .bounds
            .max
            .min(UVec2::new(grid.width, grid.height).as_ivec2() - IVec2::ONE);
        if min.cmpgt(max).any() {
            continue;
        }
        let bounds = IRect { min, max };
        work.pending = Some(work.pending.map_or(bounds, |pending| pending.union(bounds)));
        work.delay = policy.delay_seconds;
    }
}

pub(super) fn refresh_dirty_guest_influence_cells(
    mut updates: MessageReader<UpdateGuestInfluenceCells>,
    mut roots: Query<(
        &SpatialGrid,
        &GuestInfluenceUpdatePolicy,
        &mut GuestInfluenceRegionWork,
        &mut GuestInfluenceCells,
        Option<&GuestTerrainInfluence>,
    )>,
    emitters: Query<(
        Entity,
        &GlobalTransform,
        &WorldMember,
        &GuestInfluenceEmitter,
    )>,
) {
    for update in updates.read() {
        let Ok((grid, policy, mut work, mut cells, terrain)) = roots.get_mut(update.root) else {
            continue;
        };
        work.delay = (work.delay - update.delta_seconds).max(0.0);
        if work.active.is_none() && work.delay <= 0.0 {
            let Some(bounds) = work.pending.take() else {
                continue;
            };
            let mut nearby: Vec<_> = emitters
                .iter()
                .filter(|(_, _, member, _)| member.root == update.root)
                .collect();
            nearby.sort_unstable_by_key(|(entity, ..)| *entity);
            let min = grid.origin + bounds.min.as_vec2() * grid.cell_size_m;
            let max = grid.origin + (bounds.max.as_vec2() + Vec2::ONE) * grid.cell_size_m;
            let emitters: Vec<_> = nearby
                .into_iter()
                .flat_map(|(_, pose, _, emitter)| {
                    let point = pose.translation().xz();
                    emitter.0.iter().copied().filter_map(move |emission| {
                        (point.distance(point.clamp(min, max)) < emission.radius)
                            .then_some((point, emission))
                    })
                })
                .collect();
            work.active = Some(ActiveInfluenceRegion {
                bounds,
                next: bounds.min,
                emitters,
            });
        }
        let Some(active) = work.active.as_mut() else {
            continue;
        };
        for _ in 0..policy.cells_per_update {
            let coord = active.next.as_uvec2();
            let cell = coord.y * grid.width + coord.x;
            let position =
                grid.origin + (active.next.as_vec2() + Vec2::splat(0.5)) * grid.cell_size_m;
            let mut values: Vec<GuestInfluenceEntry> =
                terrain.map_or_else(Vec::new, |terrain| terrain.0.clone());
            for value in active
                .emitters
                .iter()
                .filter_map(|&(point, emission)| emission_at(emission, point.distance(position)))
            {
                if let Some(existing) = values
                    .iter_mut()
                    .find(|entry| entry.category == value.category && entry.key == value.key)
                {
                    existing.intensity += value.intensity;
                } else {
                    values.push(value);
                }
            }
            if terrain.map_or_else(|| values.is_empty(), |terrain| terrain.0 == values) {
                cells.0.remove(&cell);
            } else {
                cells.0.insert(cell, values);
            }
            if active.next.x < active.bounds.max.x {
                active.next.x += 1;
            } else if active.next.y < active.bounds.max.y {
                active.next.x = active.bounds.min.x;
                active.next.y += 1;
            } else {
                work.active = None;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::AssetId;

    #[test]
    fn dung_falls_to_half_at_three_metres_and_excludes_the_radius_boundary() {
        let emission = GuestInfluenceEmission {
            value: GuestInfluenceEntry {
                category: AssetId::from_key("smell"),
                key: AssetId::from_key("bad"),
                intensity: 0.2,
            },
            radius: 6.0,
            dropoff: InfluenceRadiusDropoff::Linear,
        };
        assert_eq!(emission_at(emission, 3.0).unwrap().intensity, 0.1);
        assert!(emission_at(emission, 6.0).is_none());
    }
}
