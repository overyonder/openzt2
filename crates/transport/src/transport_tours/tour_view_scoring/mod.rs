use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::transportation_and_tours::TourViewDefinition;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::StartBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestReaction;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};

use crate::transport_tours::tour_score_calculations::add_observation_value_to_tour_score;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourObservationCadence;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourObservationMemory;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourScore;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourViewable;
use openzt2_zoo_state::transport_tours::transport_rider_types::{
    TransportRider, TransportSeat, TransportSeatRider, VehicleRiders,
};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

fn find_vehicle_seat_emote_behavior_for_tour_score(
    definitions: WorldDefinitionsView<'_>,
    vehicle: AssetId,
    seat_index: u16,
    score: f32,
) -> Option<AssetId> {
    let seat = definitions.find_vehicle_seat_by_index(vehicle, seat_index)?;
    seat.emote_bands
        .iter()
        .find(|band| band.score[0] <= score && score < band.score[1])
        .map(|band| AssetId(band.behavior_set.0))
}

fn calculate_curved_tour_score(base: f32, curve: &[f32; 2]) -> f32 {
    let magnitude = base.abs().powf(curve[1].max(0.0));
    base.signum() * curve[0] * magnitude
}

fn calculate_deterministic_unit_value_for_guest_and_subject(
    entity: Entity,
    subject: Entity,
) -> f32 {
    let bits = entity
        .to_bits()
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        ^ subject.to_bits();
    (bits as u32) as f32 / u32::MAX as f32
}

fn calculate_deterministic_tour_score_factor(
    entity: Entity,
    subject: Entity,
    bounds: &[f32; 2],
) -> f32 {
    let unit = calculate_deterministic_unit_value_for_guest_and_subject(entity, subject);
    let minimum = bounds[0];
    minimum + (bounds[1] - minimum) * unit
}

struct TourVehicleViewCandidate<'a> {
    entity: Entity,
    target: Vec3,
    definition: &'a TourViewDefinition,
    is_animal: bool,
    shared_occluder: Option<Entity>,
}

fn first_tour_view_occluder(
    spatial_query: &SpatialQuery,
    parents: &Query<&ChildOf>,
    origin: Vec3,
    target: Vec3,
    excluded_owners: &[Entity],
) -> Option<Entity> {
    let offset = target - origin;
    let direction = Dir3::new(offset).ok()?;
    spatial_query
        .cast_ray_predicate(
            origin,
            direction,
            offset.length(),
            false,
            &SpatialQueryFilter::DEFAULT,
            &|hit| {
                !excluded_owners.contains(&hit)
                    && !parents
                        .iter_ancestors::<ChildOf>(hit)
                        .any(|ancestor| excluded_owners.contains(&ancestor))
            },
        )
        .map(|hit| hit.entity)
}

fn gather_tour_view_candidates<'a>(
    root: Entity,
    grid: &SpatialGrid,
    membership: &SpatialGridMembership,
    center: UVec2,
    cell_radius: u32,
    origin: Vec3,
    definitions: WorldDefinitionsView<'a>,
    viewables: &Query<(&TourViewable, &GlobalTransform, Has<Animal>)>,
) -> Vec<TourVehicleViewCandidate<'a>> {
    let mut candidates = Vec::new();
    let minimum = center.saturating_sub(UVec2::splat(cell_radius));
    let maximum = center
        .saturating_add(UVec2::splat(cell_radius))
        .min(UVec2::new(grid.width - 1, grid.height - 1));
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            for entity in membership.members(root, y * grid.width + x) {
                let Ok((viewable, transform, is_animal)) = viewables.get(entity) else {
                    continue;
                };
                let Some(definition) = definitions.find_tour_view(viewable.definition) else {
                    continue;
                };
                let target = transform.translation();
                let radius = definition.radius_cm as f32 * 0.01;
                if origin.distance_squared(target) <= radius * radius {
                    candidates.push(TourVehicleViewCandidate {
                        entity,
                        target,
                        definition,
                        is_animal,
                        shared_occluder: None,
                    });
                }
            }
        }
    }
    candidates
}

pub(super) fn calculate_tour_rating_from_authored_score_ranges(
    definitions: WorldDefinitionsView<'_>,
    score: f32,
) -> f32 {
    let Some(policy) = definitions.tour_scoring() else {
        return score;
    };
    policy
        .rating_ranges
        .iter()
        .find_map(|range| {
            let score_min = range.score[0];
            let score_max = range.score[1];
            if !(score_min..=score_max).contains(&score) {
                return None;
            }
            let span = score_max - score_min;
            let fraction = if span > 0.0 {
                (score - score_min) / span
            } else {
                0.0
            };
            let rating_min = range.rating[0];
            Some(rating_min + (range.rating[1] - rating_min) * fraction)
        })
        .unwrap_or(score)
}

pub(super) fn observe_visible_tour_subjects_and_accumulate_rider_scores(
    grids: Query<&SpatialGrid>,
    membership: SpatialGridMembership,
    spatial_query: SpatialQuery,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut riders: Query<
        (
            Entity,
            &TransportRider,
            &TransportSeatRider,
            &mut TourScore,
            &mut TourObservationMemory,
            &mut TourObservationCadence,
        ),
        With<Guest>,
    >,
    vehicles: Query<(
        Entity,
        &TransportVehicle,
        &GlobalTransform,
        &VehicleRiders,
        &WorldMember,
    )>,
    seats: Query<&TransportSeat>,
    viewables: Query<(&TourViewable, &GlobalTransform, Has<Animal>)>,
    parents: Query<&ChildOf>,
    mut reactions: MessageWriter<GuestReaction>,
    mut behavior: MessageWriter<StartBehaviorSet>,
) {
    if riders.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let policy = definitions.tour_scoring();
    let maximum_radius_m = definitions
        .tour_views()
        .map(|definition| definition.radius_cm as f32 * 0.01)
        .fold(0.0f32, f32::max);
    let memory_limit = policy.map_or(u16::MAX, |policy| policy.view_target_memory);
    let mut candidates_by_vehicle = EntityHashMap::default();
    for (vehicle, configuration, transform, passengers, member) in &vehicles {
        let Ok(grid) = grids.get(member.root) else {
            continue;
        };
        let cell_radius = (maximum_radius_m / grid.cell_size_m).ceil() as u32;
        let origin = transform.translation();
        let Some(center) = grid
            .cell_of(origin.xz())
            .and_then(|cell| grid.cell_xy(cell))
        else {
            continue;
        };
        let due = riders
            .iter_many(passengers.iter())
            .filter_map(Result::ok)
            .filter(|(_, _, _, score, _, cadence)| {
                cadence.ticks_until_next == 0
                    && policy
                        .is_none_or(|policy| score.observations < u32::from(policy.tour_history))
            })
            .map(|(_, _, _, _, memory, _)| memory)
            .collect::<Vec<_>>();
        if due.is_empty() {
            continue;
        }
        let mut candidates = gather_tour_view_candidates(
            member.root,
            grid,
            &membership,
            center,
            cell_radius,
            origin,
            definitions,
            &viewables,
        );
        candidates.retain_mut(|candidate| {
            if !due
                .iter()
                .any(|memory| memory.can_observe_with_limit(candidate.entity, memory_limit))
            {
                return false;
            }
            if candidate.definition.occlusion_required {
                candidate.shared_occluder = first_tour_view_occluder(
                    &spatial_query,
                    &parents,
                    origin,
                    candidate.target,
                    &[vehicle, candidate.entity],
                );
            }
            true
        });
        candidates_by_vehicle.insert(vehicle, (configuration.definition, origin, candidates));
    }

    let mut feedback =
        bevy::utils::Parallel::<Vec<(Entity, Option<StartBehaviorSet>, GuestReaction)>>::default();
    riders.par_iter_mut().for_each(
        |(guest, rider, mount, mut score, mut memory, mut cadence)| {
            if cadence.ticks_until_next > 0 {
                cadence.ticks_until_next -= 1;
                return;
            }
            let Some((vehicle_definition, origin, candidates)) =
                candidates_by_vehicle.get(&rider.vehicle)
            else {
                return;
            };
            let Ok(seat) = seats.get(mount.0) else {
                return;
            };
            let mut next_observation = 0;
            for candidate in candidates {
                if policy.is_some_and(|policy| score.observations >= u32::from(policy.tour_history))
                {
                    break;
                }
                let subject = candidate.entity;
                if !memory.can_observe_with_limit(subject, memory_limit) {
                    continue;
                }
                let definition = candidate.definition;
                let is_animal = candidate.is_animal;
                if let Some(hit) = candidate.shared_occluder {
                    let rider_owns_hit = hit == guest
                        || parents
                            .iter_ancestors::<ChildOf>(hit)
                            .any(|ancestor| ancestor == guest);
                    if !rider_owns_hit
                        || first_tour_view_occluder(
                            &spatial_query,
                            &parents,
                            *origin,
                            candidate.target,
                            &[guest, rider.vehicle, subject],
                        )
                        .is_some()
                    {
                        continue;
                    }
                }
                memory.observe_with_limit(subject, memory_limit);
                let raw_score = f32::from(definition.base_score);
                let observation_score = policy.map_or(raw_score, |policy| {
                    let event_score =
                        calculate_curved_tour_score(raw_score, &policy.view_event_curve);
                    let subject_score = if is_animal {
                        calculate_curved_tour_score(
                            raw_score + policy.species_base_inducement,
                            &policy.animal_curve,
                        )
                    } else {
                        calculate_curved_tour_score(raw_score, &policy.static_object_curve)
                    };
                    let category_bonus = {
                        policy
                            .categories
                            .iter()
                            .find(|row| row.category == definition.subject)
                            .map_or(0.0, |row| row.value as f32)
                    };
                    (event_score + subject_score + category_bonus)
                        * calculate_deterministic_tour_score_factor(
                            guest,
                            subject,
                            &policy.inducement_random_factor,
                        )
                });
                add_observation_value_to_tour_score(&mut score, observation_score);
                next_observation = next_observation.max(definition.dwell_ticks);
                let gives_feedback = policy.is_none_or(|policy| {
                    if score.value < policy.score_threshold {
                        return false;
                    }
                    if is_animal {
                        observation_score >= policy.animal_feedback_threshold
                            && calculate_deterministic_unit_value_for_guest_and_subject(
                                guest, subject,
                            ) <= policy.animal_feedback_probability
                    } else {
                        observation_score <= policy.object_feedback_thresholds[0]
                            || observation_score >= policy.object_feedback_thresholds[1]
                    }
                });
                if !gives_feedback {
                    continue;
                }
                let emote = find_vehicle_seat_emote_behavior_for_tour_score(
                    definitions,
                    *vehicle_definition,
                    seat.index,
                    observation_score,
                )
                .map(|program| StartBehaviorSet {
                    actor: guest,
                    program,
                    target: Some(subject),
                });
                let reaction = GuestReaction {
                    guest,
                    subject,
                    kind: GuestMemoryKind::Tour,
                    satisfaction_delta_permille: 0,
                    education_delta_permille: 0,
                };
                feedback.scope(|items| items.push((guest, emote, reaction)));
            }
            cadence.ticks_until_next = next_observation;
        },
    );
    let mut by_rider = EntityHashMap::<Vec<_>>::default();
    for (guest, emote, reaction) in feedback.drain() {
        by_rider.entry(guest).or_default().push((emote, reaction));
    }
    for (guest, ..) in &riders {
        for (emote, reaction) in by_rider.remove(&guest).into_iter().flatten() {
            if let Some(emote) = emote {
                behavior.write(emote);
            }
            reactions.write(reaction);
        }
    }
}
