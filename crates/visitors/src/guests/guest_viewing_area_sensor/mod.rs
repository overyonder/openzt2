pub(super) mod projection;

use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::donations::donation_guest_decision_types::GuestDonationMeters;

#[derive(Component, Default)]
pub struct GuestViewingAreaWorklist {
    pub areas: Vec<Entity>,
    pub has_viewable_candidates: bool,
    pub has_user_camera_view_candidate: bool,
    pub mode_one_visible_species_count: Option<usize>,
    next_projection_tick: u64,
}

impl GuestViewingAreaWorklist {
    pub(super) fn is_viewable(&self, has_intersections: bool) -> bool {
        has_intersections || (self.has_viewable_candidates && self.has_user_camera_view_candidate)
    }
}

#[derive(Component, Default, PartialEq)]
pub struct GuestViewingArea {
    pub supports: Vec<Entity>,
    pub intersections: Vec<Entity>,
}

#[derive(Component, PartialEq)]
pub struct GuestViewSupport {
    pub species: Option<AssetId>,
    pub view_bonus: f32,
    pub excluded_surface: bool,
}

#[derive(Component, PartialEq)]
pub struct GuestViewpointContactScores(pub Vec<i32>);

#[derive(Component, PartialEq)]
pub struct GuestViewpointSupports(pub Vec<Entity>);

#[derive(Component)]
pub struct GuestViewingSensor {
    pub apply_noise_factor: bool,
    pub entrance: Option<Entity>,
    pub favourite_score: f32,
    pub noise_factor: f32,
    pub num_to_see: usize,
    pub(super) next_capacity_refresh_tick: u64,
}

impl Default for GuestViewingSensor {
    fn default() -> Self {
        Self {
            apply_noise_factor: false,
            entrance: None,
            favourite_score: 100.0,
            noise_factor: 200.0,
            num_to_see: 2,
            next_capacity_refresh_tick: 0,
        }
    }
}

fn area_score(
    origin: Vec3,
    favourite: Option<AssetId>,
    favourite_score: f32,
    supports: impl IntoIterator<Item = (Vec3, Option<AssetId>, f32, bool)>,
) -> Option<f32> {
    let mut total = 0.0;
    let mut count = 0;
    let mut bonus: f32 = 0.0;
    let mut has_favourite = false;
    for (point, species, adjustment, excluded) in supports {
        if excluded {
            return None;
        }
        count += 1;
        total += origin.distance(point);
        bonus = bonus.max(adjustment);
        has_favourite |= favourite.is_some() && species == favourite;
    }
    (count != 0)
        .then(|| total / count as f32 - if has_favourite { favourite_score } else { 0.0 } - bonus)
}

fn collect_viewpoint_candidates(
    candidates: impl IntoIterator<Item = (Entity, i32)>,
) -> Vec<Entity> {
    let mut seen = EntityHashSet::default();
    candidates
        .into_iter()
        .filter_map(|(entity, score)| {
            (score.max(0) < 10_000 && seen.insert(entity)).then_some(entity)
        })
        .collect()
}

fn append_entrance(
    entrance: &mut Option<Entity>,
    candidates: &mut Vec<Entity>,
    entrances: &[Entity],
    draw: impl FnOnce() -> u16,
) {
    if !entrance.is_some_and(|entity| entrances.contains(&entity)) {
        *entrance =
            (!entrances.is_empty()).then(|| entrances[usize::from(draw()) % entrances.len()]);
    }
    if let Some(entity) = *entrance {
        if !candidates.contains(&entity) {
            candidates.push(entity);
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct ViewingAreaSensors<'w, 's> {
    pub(super) worklists: Query<'w, 's, &'static GuestViewingAreaWorklist>,
    areas: Query<'w, 's, &'static GuestViewingArea>,
    supports: Query<'w, 's, (&'static GlobalTransform, &'static GuestViewSupport)>,
    contacts: Query<'w, 's, &'static GuestViewpointContactScores>,
    entrances: Query<
        'w,
        's,
        (Entity, &'static WorldMember),
        With<openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance>,
    >,
}

impl ViewingAreaSensors<'_, '_> {
    pub(super) fn has_intersections(&self, worklist: &GuestViewingAreaWorklist) -> bool {
        worklist.areas.iter().any(|&area| {
            self.areas
                .get(area)
                .is_ok_and(|area| !area.intersections.is_empty())
        })
    }

    pub(super) fn candidates(
        &self,
        world: Entity,
        origin: Vec3,
        favourite: Option<AssetId>,
        history: Option<&GuestDonationMeters>,
        state: &mut GuestViewingSensor,
        mut draw: impl FnMut() -> u16,
    ) -> Vec<Entity> {
        let Ok(worklist) = self.worklists.get(world) else {
            return Vec::new();
        };
        let mut best_score = 1_000_000_000.0;
        let mut best_area = None;
        for area in worklist
            .areas
            .iter()
            .filter_map(|&entity| self.areas.get(entity).ok())
        {
            if area.intersections.is_empty() {
                continue;
            }
            let supports = self.supports.iter_many(area.supports.iter().copied()).filter_map(|result| {
                let (position, support) = result.ok()?;
                let seen = support.species.is_some_and(|species| {
                    history.is_some_and(|history| history.0.get(&species).is_some_and(|&score| {
                        score == crate::donations::donation_opportunity_observation::SPENT_SPECIES_SCORE
                    }))
                });
                (!seen || support.excluded_surface).then_some((
                    position.translation(), support.species, support.view_bonus, support.excluded_surface,
                ))
            });
            let Some(mut score) = area_score(origin, favourite, state.favourite_score, supports)
            else {
                continue;
            };
            if state.apply_noise_factor {
                score += f32::from(draw()) / 32_767.0 * state.noise_factor;
            }
            if score < best_score {
                best_score = score;
                best_area = Some(area);
            }
        }
        state.apply_noise_factor = false;
        let mut candidates = collect_viewpoint_candidates(best_area.into_iter().flat_map(|area| {
            area.intersections.iter().filter_map(|&candidate| {
                let contacts = self.contacts.get(candidate).ok()?;
                Some((candidate, contacts.0.iter().copied().max().unwrap_or(0)))
            })
        }));
        let entrances: Vec<_> = self
            .entrances
            .iter()
            .filter_map(|(entity, member)| (member.root == world).then_some(entity))
            .collect();
        append_entrance(&mut state.entrance, &mut candidates, &entrances, draw);
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favourite_bonus_applies_once_and_equal_areas_keep_first() {
        let tiger = AssetId::from_key("tiger");
        let ordinary = (Vec3::X * 25.0, None, 0.0, false);
        let favourite = (Vec3::X * 25.0, Some(tiger), 0.0, false);
        let score = |support| area_score(Vec3::ZERO, Some(tiger), 100.0, [support]).unwrap();
        assert_eq!(score(ordinary), 25.0);
        assert_eq!(score(favourite), -75.0);
        let result = [ordinary, favourite, favourite]
            .into_iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| score(*left).total_cmp(&score(*right)));
        assert_eq!(result.map(|(index, _)| index), Some(1));
    }

    #[test]
    fn collision_cutoff_is_strict_and_passed_candidates_are_unique() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        assert_eq!(
            collect_viewpoint_candidates([(a, 9999), (b, 10000), (a, 9999)]),
            [a]
        );
    }

    #[test]
    fn missing_area_still_offers_one_entrance_and_retains_it_until_invalid() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let mut entrance = None;
        let mut output = Vec::new();
        append_entrance(&mut entrance, &mut output, &[a, b], || 1);
        assert_eq!(output, [b]);
        output.clear();
        append_entrance(&mut entrance, &mut output, &[a, b], || {
            panic!("cached entrance drew")
        });
        assert_eq!(output, [b]);
        output.clear();
        append_entrance(&mut entrance, &mut output, &[], || {
            panic!("no entrance drew")
        });
        assert!(output.is_empty());
    }
}
