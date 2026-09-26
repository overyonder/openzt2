use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::animal_behavior::behavior_set_start_request_types::StartBehaviorSet;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::guests::guest_simulation_types::GuestReaction;
use crate::plugins::locomotion::locomotion_types::SpatialGrid;

use super::{
    tour_score_calculations::add_observation_value_to_tour_score,
    tour_scoring_types::{TourObservationCadence, TourObservationMemory, TourScore, TourViewable},
    transport_hierarchy_queries::entity_is_descendant_of_transport_owner,
    transport_rider_types::TransportRider,
    transport_vehicle_types::TransportVehicle,
};

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

fn tour_subject_is_visible_from_transport_vehicle(
    spatial_query: &SpatialQuery,
    parents: &Query<&ChildOf>,
    origin: Vec3,
    target: Vec3,
    guest: Entity,
    vehicle: Entity,
    subject: Entity,
) -> bool {
    let offset = target - origin;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return true;
    };
    spatial_query
        .cast_ray_predicate(
            origin,
            direction,
            distance,
            false,
            &SpatialQueryFilter::DEFAULT,
            &|hit| {
                !entity_is_descendant_of_transport_owner(hit, guest, parents)
                    && !entity_is_descendant_of_transport_owner(hit, vehicle, parents)
                    && !entity_is_descendant_of_transport_owner(hit, subject, parents)
            },
        )
        .is_none()
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
    grid: Res<SpatialGrid>,
    spatial_query: SpatialQuery,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut riders: Query<
        (
            Entity,
            &TransportRider,
            &mut TourScore,
            &mut TourObservationMemory,
            &mut TourObservationCadence,
        ),
        With<Guest>,
    >,
    vehicles: Query<(&TransportVehicle, &GlobalTransform)>,
    viewables: Query<(Entity, &TourViewable, &GlobalTransform, Option<&Animal>)>,
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
    for (guest, rider, mut score, mut memory, mut cadence) in &mut riders {
        if cadence.ticks_until_next > 0 {
            cadence.ticks_until_next -= 1;
            continue;
        }
        let Ok((vehicle, vehicle_transform)) = vehicles.get(rider.vehicle) else {
            continue;
        };
        let origin = vehicle_transform.translation();
        let Some(center) = grid.cell_of(origin.xz()) else {
            continue;
        };
        let Some(center_xy) = grid.cell_xy(center) else {
            continue;
        };
        let cell_radius = (maximum_radius_m / grid.cell_size_m).ceil() as u32;
        let mut next_observation = 0u32;
        for y in center_xy.y.saturating_sub(cell_radius)
            ..=(center_xy.y + cell_radius).min(grid.height - 1)
        {
            for x in center_xy.x.saturating_sub(cell_radius)
                ..=(center_xy.x + cell_radius).min(grid.width - 1)
            {
                for &subject in grid.range(y * grid.width + x) {
                    let Ok((_, viewable, transform, animal)) = viewables.get(subject) else {
                        continue;
                    };
                    let Some(definition) = definitions.find_tour_view(viewable.definition) else {
                        continue;
                    };
                    if definition.occlusion_required
                        && !tour_subject_is_visible_from_transport_vehicle(
                            &spatial_query,
                            &parents,
                            origin,
                            transform.translation(),
                            guest,
                            rider.vehicle,
                            subject,
                        )
                    {
                        continue;
                    }
                    let radius_m = definition.radius_cm as f32 * 0.01;
                    if policy
                        .is_some_and(|policy| score.observations >= u32::from(policy.tour_history))
                    {
                        continue;
                    }
                    let memory_limit = policy.map_or(u16::MAX, |policy| policy.view_target_memory);
                    if origin.distance_squared(transform.translation()) > radius_m * radius_m
                        || !memory.observe_with_limit(subject, memory_limit)
                    {
                        continue;
                    }
                    let raw_score = f32::from(definition.base_score);
                    let observation_score = policy.map_or(raw_score, |policy| {
                        let event_score =
                            calculate_curved_tour_score(raw_score, &policy.view_event_curve);
                        let subject_score = if animal.is_some() {
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
                    let feedback = policy.is_none_or(|policy| {
                        if score.value < policy.score_threshold {
                            return false;
                        }
                        if animal.is_some() {
                            observation_score >= policy.animal_feedback_threshold
                                && calculate_deterministic_unit_value_for_guest_and_subject(
                                    guest, subject,
                                ) <= policy.animal_feedback_probability
                        } else {
                            observation_score <= policy.object_feedback_thresholds[0]
                                || observation_score >= policy.object_feedback_thresholds[1]
                        }
                    });
                    if !feedback {
                        continue;
                    }
                    if let Some(program) = find_vehicle_seat_emote_behavior_for_tour_score(
                        definitions,
                        vehicle.definition,
                        rider.seat_index,
                        observation_score,
                    ) {
                        behavior.write(StartBehaviorSet {
                            actor: guest,
                            program,
                            target: Some(subject),
                        });
                    }
                    reactions.write(GuestReaction {
                        guest,
                        subject,
                        kind: GuestMemoryKind::Tour,
                        satisfaction_delta_permille: 0,
                        education_delta_permille: 0,
                    });
                }
            }
        }
        cadence.ticks_until_next = next_observation;
    }
}
