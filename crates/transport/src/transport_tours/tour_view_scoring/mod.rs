use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::transportation_and_tours::TourViewDefinition;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::GuestTourDepartureGate;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::StartBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestReaction;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};

use crate::transport_tours::tour_score_calculations::add_observation_value_to_tour_score;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourObservationMemory;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourScore;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourViewable;
use openzt2_zoo_state::transport_tours::transport_rider_types::{
    TransportRider, TransportSeat, TransportSeatRider, VehicleRiders,
};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

#[derive(Component, Default)]
pub(super) struct TourObservationDwell(Option<Timer>);

#[derive(Component, Default)]
pub(super) struct RideTourContributions(pub(super) [f32; 2]);

/// The visitor's tour state in authored source units.

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

struct TourVehicleViewCandidate<'a> {
    entity: Entity,
    target: Vec3,
    definition: &'a TourViewDefinition,
    is_animal: bool,
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
                candidates.push(TourVehicleViewCandidate {
                    entity,
                    target,
                    definition,
                    is_animal,
                });
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
    fixed_time: Res<Time<Fixed>>,
    view_catalogue: Res<super::transport_definition_hydration::TourViewCatalogue>,
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
            &mut TourObservationDwell,
            &mut GuestRng,
            &mut RideTourContributions,
            Option<&GuestTourDepartureGate>,
        ),
        With<Guest>,
    >,
    vehicles: Query<(
        Entity,
        &TransportVehicle,
        &Transform,
        &VehicleRiders,
        &WorldMember,
    )>,
    seats: Query<(&TransportSeat, &Transform)>,
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
    let memory_limit = policy.map_or(u16::MAX, |policy| policy.view_target_memory);
    let mut candidates_by_vehicle = EntityHashMap::default();
    for (vehicle, configuration, transform, passengers, member) in &vehicles {
        let Ok(grid) = grids.get(member.root) else {
            continue;
        };
        let origin = transform.translation;
        let Some(center) = grid
            .cell_of(origin.xz())
            .and_then(|cell| grid.cell_xy(cell))
        else {
            continue;
        };
        let seat_radius = riders
            .iter_many(passengers.iter())
            .filter_map(Result::ok)
            .filter(|(_, _, _, score, _, cadence, ..)| {
                cadence.0.as_ref().is_none_or(Timer::is_finished)
                    && policy
                        .is_none_or(|policy| score.observations < u32::from(policy.tour_history))
            })
            .filter_map(|(_, _, mount, ..)| seats.get(mount.0).ok())
            .map(|(_, local)| (transform.scale * local.translation).length())
            .reduce(f32::max);
        let Some(seat_radius) = seat_radius else {
            continue;
        };
        let cell_radius =
            ((view_catalogue.maximum_radius_m + seat_radius) / grid.cell_size_m).ceil() as u32;
        let candidates = gather_tour_view_candidates(
            member.root,
            grid,
            &membership,
            center,
            cell_radius,
            definitions,
            &viewables,
        );
        candidates_by_vehicle.insert(vehicle, (configuration.definition, *transform, candidates));
    }

    let mut feedback =
        bevy::utils::Parallel::<Vec<(Entity, Option<StartBehaviorSet>, GuestReaction)>>::default();
    let observation_step = fixed_time.timestep();
    riders.par_iter_mut().for_each(
        |(
            guest,
            rider,
            mount,
            mut score,
            mut memory,
            mut cadence,
            mut rng,
            mut contributions,
            need,
        )| {
            if let Some(timer) = cadence.0.as_mut().filter(|timer| !timer.is_finished()) {
                timer.tick(observation_step);
                return;
            }
            let Some((vehicle_definition, vehicle_pose, candidates)) =
                candidates_by_vehicle.get(&rider.vehicle)
            else {
                return;
            };
            let Ok((seat, local_seat)) = seats.get(mount.0) else {
                return;
            };
            let authored_seat =
                definitions.find_vehicle_seat_by_index(*vehicle_definition, seat.index);
            let seat_pose = super::transport_passenger_mounts::occupied_seat_pose(
                vehicle_pose,
                local_seat,
                seat.back_facing,
            );
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
                let offset = candidate.target - seat_pose.translation;
                let radius = definition.radius_cm as f32 * 0.01;
                if offset.length_squared() > radius * radius
                    || authored_seat.is_some_and(|seat| {
                        !super::transport_passenger_mounts::seat_can_view_target(
                            seat,
                            &seat_pose,
                            candidate.target,
                        )
                    })
                    || definition.occlusion_required
                        && first_tour_view_occluder(
                            &spatial_query,
                            &parents,
                            seat_pose.translation,
                            candidate.target,
                            &[guest, rider.vehicle, subject],
                        )
                        .is_some()
                {
                    continue;
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
                    let [minimum, maximum] = policy.inducement_random_factor;
                    let factor = minimum + (maximum - minimum) * rng.0.unit_f32();
                    contributions.0[0] += event_score * factor;
                    contributions.0[1] += (subject_score + category_bonus) * factor;
                    (event_score + subject_score + category_bonus) * factor
                });
                add_observation_value_to_tour_score(&mut score, observation_score);
                next_observation = next_observation.max(definition.dwell_ticks);
                let gives_feedback = policy.is_none_or(|policy| {
                    if score.value < policy.score_threshold {
                        return false;
                    }
                    if need.is_some_and(|need| need.need < need.threshold) {
                        return false;
                    }
                    if is_animal {
                        observation_score >= policy.animal_feedback_threshold
                            && rng.0.range_u32(100).is_some_and(|roll| {
                                (roll as f32) < policy.animal_feedback_probability
                            })
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
            cadence.0 = (next_observation > 0).then(|| {
                Timer::new(
                    observation_step.saturating_mul(next_observation),
                    TimerMode::Once,
                )
            });
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
