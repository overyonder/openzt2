use std::io;

use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::ecs::entity::EntityHashMap;
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::transportation_and_tours::{
    TourRatingRange, TourViewDefinition, TransportationTrackKind,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::PersistentId, world_membership_types::WorldMember,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    GuestTourDepartureGate, RunBehaviorTour,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::StartBehaviorSet;
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal,
    animal_welfare::types::NeedPoints,
    authored_float_attributes::AuthoredFloatAttributes,
};
use openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity;
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

#[derive(Asset, TypePath)]
pub(super) struct SkyTourViewRange(f32);

#[derive(Default, TypePath)]
pub(super) struct SkyTourViewRangeLoader;

impl AssetLoader for SkyTourViewRangeLoader {
    type Asset = SkyTourViewRange;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let source = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        source
            .root
            .attribute_named_any(&["viewingRangeSky", "viewingRange"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|range| range.is_finite() && *range >= 0.0)
            .map(SkyTourViewRange)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid sky viewing range"))
    }
}

#[derive(Resource)]
pub(super) struct SkyTourViewRangeSource(Handle<SkyTourViewRange>);

impl FromWorld for SkyTourViewRangeSource {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource::<AssetServer>()
                .load("config/traversability.xml"),
        )
    }
}

pub(super) fn authored_maximum_animal_tour_view() -> AssetId {
    AssetId::from_key("f_maxtourviewvalue")
}

pub(super) fn authored_static_tour_value() -> AssetId {
    AssetId::from_key("zt_tour_object_value")
}

fn authored_float(
    definitions: WorldDefinitionsView<'_>,
    object: AssetId,
    key: AssetId,
) -> Option<f32> {
    definitions
        .find_object(object)?
        .authored_runtime
        .numeric_behavior_properties
        .iter()
        .find_map(|&(name, value)| (name == key).then_some(value))
}

#[derive(Component, Default)]
pub(super) struct TourObservationDwell(Option<Timer>);

#[derive(Component, Default)]
pub(super) struct RideTourContributions(pub(super) [f32; 2]);

/// Seated feedback that plays as a child of the rider's active tour.
#[derive(Message)]
pub struct TourSeatFeedbackRequested {
    pub rider: Entity,
    pub subject: Entity,
    pub program: AssetId,
}

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
    let magnitude = (base.abs() * curve[0]).powf(curve[1]);
    base.signum() * magnitude
}

struct TourVehicleViewCandidate<'a> {
    entity: Entity,
    target: Vec3,
    definition: Option<&'a TourViewDefinition>,
    native_score: f32,
    is_animal: bool,
    persistent_order: u64,
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

type TourViewTargets<'w, 's> = Query<
    'w,
    's,
    (
        &'static TourViewable,
        &'static GlobalTransform,
        Has<Animal>,
        Option<&'static NeedPoints>,
        Option<&'static AuthoredFloatAttributes>,
        Option<&'static PersistentId>,
    ),
    UncratedWorldEntity,
>;

fn gather_tour_view_candidates<'a>(
    root: Entity,
    grid: &SpatialGrid,
    membership: &SpatialGridMembership,
    center: UVec2,
    cell_radius: u32,
    definitions: WorldDefinitionsView<'a>,
    viewables: &TourViewTargets,
    animal_vehicle_multiplier: f32,
) -> Vec<TourVehicleViewCandidate<'a>> {
    let mut candidates = Vec::new();
    let animal_maximum = authored_maximum_animal_tour_view();
    let static_value = authored_static_tour_value();
    let minimum = center.saturating_sub(UVec2::splat(cell_radius));
    let maximum = center
        .saturating_add(UVec2::splat(cell_radius))
        .min(UVec2::new(grid.width - 1, grid.height - 1));
    for y in minimum.y..=maximum.y {
        for x in minimum.x..=maximum.x {
            for entity in membership.members(root, y * grid.width + x) {
                let Ok((viewable, transform, is_animal, needs, attributes, persistent)) =
                    viewables.get(entity)
                else {
                    continue;
                };
                let definition = definitions.find_tour_view(viewable.definition);
                let key = if is_animal {
                    animal_maximum
                } else {
                    static_value
                };
                let mut native_score = attributes
                    .and_then(|attributes| attributes.get(key))
                    .or_else(|| authored_float(definitions, viewable.definition, key))
                    .unwrap_or_default();
                if is_animal {
                    let need_points = needs.map_or(0.0, |needs| {
                        if needs.good < needs.bad.abs() {
                            -needs.bad.abs()
                        } else {
                            needs.good
                        }
                    });
                    native_score *=
                        need_points.clamp(-100.0, 100.0) * (animal_vehicle_multiplier / 100.0);
                }
                if definition.is_none() && (!native_score.is_finite() || native_score == 0.0) {
                    continue;
                }
                let target = transform.translation();
                candidates.push(TourVehicleViewCandidate {
                    entity,
                    target,
                    definition,
                    native_score,
                    is_animal,
                    persistent_order: persistent.map_or(u64::MAX, |id| id.0),
                });
            }
        }
    }
    candidates.sort_unstable_by_key(|candidate| (candidate.persistent_order, candidate.entity));
    candidates
}

pub(super) fn calculate_tour_rating_from_authored_score_ranges(
    ranges: &[TourRatingRange],
    score: f32,
) -> (f32, [f32; 2]) {
    let mut rating = score;
    let mut limits = [f32::INFINITY, f32::NEG_INFINITY];
    let mut output = [0.0, 100.0];
    for row in ranges {
        let [min, max] = row.score;
        if (min..max).contains(&score) {
            rating =
                row.rating[0] + (row.rating[1] - row.rating[0]) * ((score - min) / (max - min));
        }
        if min < limits[0] {
            limits[0] = min;
            output[0] = row.rating[0];
        }
        if max >= limits[1] {
            limits[1] = max;
            output[1] = row.rating[1];
        }
    }
    if !ranges.is_empty() {
        if rating > limits[1] {
            rating = output[1];
        }
        if rating < limits[0] {
            rating = output[0];
        }
    }
    (rating, output)
}

pub(super) fn normalized_tour_rating(rating: f32, [min, max]: [f32; 2]) -> f32 {
    if max > min {
        ((rating - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(super) fn observe_visible_tour_subjects_and_accumulate_rider_scores(
    fixed_time: Res<Time<Fixed>>,
    view_catalogue: Res<super::transport_definition_hydration::TourViewCatalogue>,
    (sky_range, sky_ranges): (Res<SkyTourViewRangeSource>, Res<Assets<SkyTourViewRange>>),
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
            Has<RunBehaviorTour>,
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
    viewables: TourViewTargets,
    parents: Query<&ChildOf>,
    mut reactions: MessageWriter<GuestReaction>,
    (mut behavior, mut tour_feedback): (
        MessageWriter<StartBehaviorSet>,
        MessageWriter<TourSeatFeedbackRequested>,
    ),
) {
    if riders.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let policy = definitions.tour_scoring();
    let ground_range = definitions
        .guest_viewing()
        .map_or(0.0, |viewing| viewing.ground_range_m);
    let sky_range = sky_ranges.get(&sky_range.0).map_or(0.0, |range| range.0);
    let animal_multiplier_key = AssetId::from_key("f_animaltourvaluemultiplier");
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
            .filter(|(_, _, _, _, _, cadence, ..)| {
                cadence.0.as_ref().is_none_or(Timer::is_finished)
            })
            .filter_map(|(_, _, mount, ..)| seats.get(mount.0).ok())
            .map(|(_, local)| (transform.scale * local.translation).length())
            .reduce(f32::max);
        let Some(seat_radius) = seat_radius else {
            continue;
        };
        let Some(vehicle_definition) = definitions.find_vehicle(configuration.definition) else {
            continue;
        };
        let native_range = match vehicle_definition.kind {
            TransportationTrackKind::Ground => ground_range,
            TransportationTrackKind::Sky => sky_range,
        };
        let cell_radius = ((view_catalogue.maximum_radius_m.max(native_range) + seat_radius)
            / grid.cell_size_m)
            .ceil() as u32;
        let candidates = gather_tour_view_candidates(
            member.root,
            grid,
            &membership,
            center,
            cell_radius,
            definitions,
            &viewables,
            authored_float(
                definitions,
                vehicle_definition.object,
                animal_multiplier_key,
            )
            .unwrap_or(1.0),
        );
        candidates_by_vehicle.insert(
            vehicle,
            (
                configuration.definition,
                *transform,
                native_range,
                candidates,
            ),
        );
    }

    let mut feedback = bevy::utils::Parallel::<Vec<_>>::default();
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
            authored_ride,
        )| {
            if let Some(timer) = cadence.0.as_mut().filter(|timer| !timer.is_finished()) {
                timer.tick(observation_step);
                return;
            }
            let Some((vehicle_definition, vehicle_pose, native_range, candidates)) =
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
            let mut emotes = Vec::new();
            for candidate in candidates {
                let subject = candidate.entity;
                if !memory.can_observe_with_limit(subject, memory_limit) {
                    continue;
                }
                let definition = candidate.definition;
                let is_animal = candidate.is_animal;
                let offset = candidate.target - seat_pose.translation;
                let radius = definition.map_or(*native_range, |definition| {
                    definition.radius_cm as f32 * 0.01
                });
                if offset.xz().length_squared() > radius * radius
                    || authored_seat.is_some_and(|seat| {
                        !super::transport_passenger_mounts::seat_can_view_target(
                            seat,
                            &seat_pose,
                            candidate.target,
                        )
                    })
                    || definition.is_none_or(|definition| definition.occlusion_required)
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
                let raw_score = definition.map_or(candidate.native_score, |definition| {
                    f32::from(definition.base_score)
                });
                let observation_score = policy.map_or(raw_score, |policy| {
                    let event_score = if definition.is_some() {
                        calculate_curved_tour_score(raw_score, &policy.view_event_curve)
                    } else {
                        0.0
                    };
                    let subject_score = if is_animal {
                        calculate_curved_tour_score(raw_score, &policy.animal_curve)
                    } else {
                        calculate_curved_tour_score(raw_score, &policy.static_object_curve)
                    };
                    let category_bonus = {
                        policy
                            .categories
                            .iter()
                            .find(|row| {
                                definition
                                    .is_some_and(|definition| row.category == definition.subject)
                            })
                            .map_or(0.0, |row| row.value as f32)
                    };
                    let [minimum, maximum] = policy.inducement_random_factor;
                    let factor = minimum + (maximum - minimum) * rng.0.unit_f32();
                    contributions.0[0] += event_score * factor;
                    contributions.0[1] += (subject_score + category_bonus) * factor;
                    (event_score + subject_score + category_bonus) * factor
                });
                add_observation_value_to_tour_score(&mut score, observation_score);
                next_observation =
                    next_observation.max(definition.map_or(0, |definition| definition.dwell_ticks));
                let gives_feedback = policy.is_none_or(|policy| {
                    if score.value < policy.score_threshold {
                        return false;
                    }
                    if need.is_some_and(|need| need.need < policy.need_threshold) {
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
                let program = find_vehicle_seat_emote_behavior_for_tour_score(
                    definitions,
                    *vehicle_definition,
                    seat.index,
                    observation_score,
                );
                let weight = observation_score * observation_score;
                if weight.is_finite() && weight > 0.0 {
                    emotes.push((weight, subject, program));
                }
            }
            if !emotes.is_empty() {
                let total = emotes.iter().map(|(weight, ..)| weight).sum::<f32>();
                let mut roll = rng.0.unit_f32() * total;
                let &(.., subject, program) = emotes
                    .iter()
                    .find(|(weight, ..)| {
                        roll -= weight;
                        roll < 0.0
                    })
                    .unwrap_or_else(|| emotes.last().expect("nonempty tour feedback"));
                feedback.scope(|items| items.push((guest, subject, program, authored_ride)));
            }
            cadence.0 = (next_observation > 0).then(|| {
                Timer::new(
                    observation_step.saturating_mul(next_observation),
                    TimerMode::Once,
                )
            });
        },
    );
    let by_rider: EntityHashMap<_> = feedback
        .drain()
        .map(|(guest, subject, program, authored_ride)| (guest, (subject, program, authored_ride)))
        .collect();
    for (guest, ..) in &riders {
        let Some(&(subject, program, authored_ride)) = by_rider.get(&guest) else {
            continue;
        };
        if let Some(program) = program {
            if authored_ride {
                tour_feedback.write(TourSeatFeedbackRequested {
                    rider: guest,
                    subject,
                    program,
                });
            } else {
                behavior.write(StartBehaviorSet {
                    actor: guest,
                    program,
                    target: Some(subject),
                });
            }
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
