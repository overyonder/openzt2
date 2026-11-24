use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_game_data::AssetId;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;
use openzt2_zoo_state::economy::service_types::{PendingServiceRequest, ServiceRequestSequence};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};
use openzt2_zoo_state::maintenance::maintenance_types::AnimalHabitatWaste;
use openzt2_zoo_state::maintenance::maintenance_types::LooseLitterWaste;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use super::guest_simulation_calculations::reached_action;
use super::guest_simulation_calculations::ReachedAction;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestAmusement;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestDestination;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestFavouriteAnimal;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestNavigationRequest;
use super::guest_simulation_types::GuestPhase;
use super::guest_simulation_types::GuestReaction;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestRng;
use super::guest_simulation_types::GuestSocial;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingTarget;
use super::guest_simulation_types::Viewing;
use super::guest_simulation_types::ViewingOpportunity;

use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::service_types::{FacilityReservations, ServiceReservation};

use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::find_reaction;
use super::guest_definition_queries::guest_need_rows;
use super::guest_definition_queries::need_value;
use super::guest_definition_queries::reconsider_threshold;
use super::guest_definition_queries::supports_purpose;

use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn choose_guest_destination(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Single<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    mut guests: Query<
        (
            Entity,
            &GlobalTransform,
            &GuestArchetype,
            &GuestHunger,
            &GuestThirst,
            &GuestDessert,
            &GuestGift,
            &GuestEnergy,
            &GuestRestroom,
            &GuestSocial,
            &GuestAmusement,
            &GuestFavouriteAnimal,
            &mut GuestRng,
            &GuestPhase,
            Option<&openzt2_zoo_state::guests::guest_simulation_types::GuestDeparturePoints>,
            &WorldMember,
        ),
        (
            With<Guest>,
            Without<GuestDestination>,
            Without<ServiceReservation>,
            Without<BehaviorTaskExecutionState>,
            Without<Viewing>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
        ),
    >,
    inspectables: Query<(
        Entity,
        &GlobalTransform,
        &Inspectable,
        Option<&ServiceFacility>,
        Option<&FacilityReservations>,
        &WorldMember,
    )>,
    transport_stations: Query<&TransportStation>,
    viewing_areas: Query<(Entity, &GlobalTransform, &ViewingGroundSlots, &WorldMember)>,
    animals: Query<(&GlobalTransform, &Visibility, &SpeciesHandle, &WorldMember), With<Animal>>,
    mut commands: Commands,
    mut guest_requests: Query<&mut GuestNavigationRequest>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut navigate: MessageWriter<NavigateTo>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (
        guest,
        guest_transform,
        archetype,
        hunger,
        thirst,
        dessert,
        gift,
        energy,
        restroom,
        social,
        amusement,
        favourite,
        mut rng,
        phase,
        departure_points,
        world,
    ) in &mut guests
    {
        // Admission and departure own navigation until the guest is visiting.
        // Service reservations and authored tasks retain it until completion.
        if *phase != GuestPhase::Visiting || departure_points.is_some_and(|points| points.0 <= 0) {
            continue;
        }
        let Some((asset, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let weights = &definition.destination_weights;
        let Some(needs) = guest_need_rows(definition) else {
            continue;
        };
        let origin = guest_transform.translation();
        let Some(origin_cell) = grid.cell_of(origin.xz()) else {
            continue;
        };
        let Some(cell_xy) = grid.cell_xy(origin_cell) else {
            continue;
        };
        let mut selected = None;
        let mut viewing_target = None;
        let mut total_weight = 0_u64;
        for weight in weights {
            let need = need_value(
                &weight.need,
                hunger,
                thirst,
                dessert,
                gift,
                energy,
                restroom,
                social,
                amusement,
            );
            let pressure = 1_000_u16.saturating_sub(need);
            let threshold = weight
                .minimum_need
                .max(reconsider_threshold(needs, &weight.need));
            let base = weight.weight;
            if pressure < threshold || base <= 0 {
                continue;
            }
            let radius_m = weight.radius_cm as f32 * 0.01;
            let cells = (radius_m / grid.cell_size_m).ceil() as u32;
            let purpose = weight.purpose;
            if purpose == GuestVisitPurpose::View {
                if let Some((choice, viewing_weight)) = choose_viewing_opportunity(
                    asset,
                    guest,
                    origin,
                    &grid,
                    &spatial_query,
                    &parents,
                    &spatial_membership,
                    world.root,
                    radius_m,
                    &viewing_areas,
                    &animals,
                    favourite.0,
                    definition.preferred_animal_view_factor_q16,
                    &mut rng.0,
                    u64::from(base as u16)
                        * u64::from(pressure.saturating_sub(threshold).saturating_add(1)),
                ) {
                    total_weight = total_weight.saturating_add(viewing_weight);
                    if total_weight != 0
                        && u64::from(rng.0.next_u32()) % total_weight < viewing_weight
                    {
                        selected = Some((choice.area, choice.stand_position, purpose));
                        viewing_target = Some(GuestViewingTarget {
                            subject: choice.subject,
                            stand_position: choice.stand_position,
                        });
                    }
                }
                continue;
            }
            let mut candidates: Vec<_> = inspectables
                .iter()
                .filter_map(|candidate| {
                    let (entity, transform, _, _, _, member) = candidate;
                    if entity == guest || member.root != world.root {
                        return None;
                    }
                    let cell = grid.cell_xy(grid.cell_of(transform.translation().xz())?)?;
                    (cell.x.abs_diff(cell_xy.x) <= cells && cell.y.abs_diff(cell_xy.y) <= cells)
                        .then_some((cell.y, cell.x, entity.to_bits(), candidate))
                })
                .collect();
            candidates.sort_unstable_by_key(|(row, column, id, _)| (*row, *column, *id));
            for (_, _, _, (entity, transform, inspectable, facility, reservations, _)) in candidates
            {
                if !supports_purpose(asset, inspectable.definition, purpose)
                    || facility.is_some_and(|facility| {
                        reservations.map_or(0, RelationshipTarget::len)
                            >= usize::from(facility.capacity)
                    })
                    || transform.translation().xz().distance_squared(origin.xz())
                        > radius_m * radius_m
                {
                    continue;
                }
                let destination = if purpose == GuestVisitPurpose::Tour {
                    let Ok(station) = transport_stations.get(entity) else {
                        continue;
                    };
                    let Some(station_definition) = asset.find_station(station.definition) else {
                        continue;
                    };
                    let Some(entry) = station_definition
                        .passenger_entry_offsets_cm
                        .into_iter()
                        .map(|[x, z]| Vec3::new(f32::from(x), 0.0, f32::from(z)) * 0.01)
                        .map(|offset| transform.transform_point(offset))
                        .min_by(|left, right| {
                            left.distance_squared(origin)
                                .total_cmp(&right.distance_squared(origin))
                        })
                    else {
                        continue;
                    };
                    entry
                } else {
                    transform.translation()
                };
                let candidate_weight = u64::from(base as u16)
                    * u64::from(pressure.saturating_sub(threshold).saturating_add(1));
                total_weight = total_weight.saturating_add(candidate_weight);
                if total_weight != 0
                    && u64::from(rng.0.next_u32()) % total_weight < candidate_weight
                {
                    selected = Some((entity, destination, purpose));
                    viewing_target = None;
                }
            }
        }
        if let Some((entity, destination, purpose)) = selected {
            let Ok(mut generation) = navigation_generations.get_mut(guest) else {
                continue;
            };
            let request_id = generation.advance();
            let mut entity_commands = commands.entity(guest);
            entity_commands.insert(GuestDestination { entity, purpose });
            let request = GuestNavigationRequest(Some(request_id));
            if let Ok(mut existing) = guest_requests.get_mut(guest) {
                existing.set_if_neq(request);
            } else {
                entity_commands.insert(request);
            }
            if let Some(target) = viewing_target {
                entity_commands.insert(target);
            }
            navigate.write(NavigateTo {
                entity: guest,
                request_id,
                destination,
                arrival_radius_m: f32::from(definition.radius_cm) * 0.01,
            });
        }
    }
}

#[derive(Clone, Copy)]
struct ViewingChoice {
    area: Entity,
    subject: Entity,
    stand_position: Vec3,
}

#[allow(clippy::too_many_arguments)]
fn choose_viewing_opportunity(
    asset: WorldDefinitionsView<'_>,
    guest: Entity,
    guest_position: Vec3,
    grid: &SpatialGrid,
    spatial_query: &SpatialQuery,
    parents: &Query<&ChildOf>,
    spatial_membership: &SpatialGridMembership,
    world: Entity,
    authored_radius: f32,
    viewing_areas: &Query<(Entity, &GlobalTransform, &ViewingGroundSlots, &WorldMember)>,
    animals: &Query<(&GlobalTransform, &Visibility, &SpeciesHandle, &WorldMember), With<Animal>>,
    favourite: Option<AssetId>,
    preferred_animal_view_factor_q16: u32,
    rng: &mut DeterministicRng,
    base_weight: u64,
) -> Option<(ViewingChoice, u64)> {
    let policy = asset.guest_viewing()?;
    let maximum_range = policy.ground_range_m.min(authored_radius);
    let animal_range = policy.ground_range_m;
    let eye_height = policy.guest_eye_height_m;
    let mut selected = None;
    let mut total_weight = 0_u64;

    let guest_cell = grid.cell_of(guest_position.xz())?;
    let guest_cell_xy = grid.cell_xy(guest_cell)?;
    let area_cells = (maximum_range / grid.cell_size_m).ceil() as u32;
    let mut areas: Vec<_> = viewing_areas
        .iter()
        .filter_map(|(area, transform, slots, member)| {
            if member.root != world {
                return None;
            }
            let cell = grid.cell_xy(grid.cell_of(transform.translation().xz())?)?;
            (cell.x.abs_diff(guest_cell_xy.x) <= area_cells
                && cell.y.abs_diff(guest_cell_xy.y) <= area_cells)
                .then_some((cell.y, cell.x, area.to_bits(), area, slots))
        })
        .collect();
    areas.sort_unstable_by_key(|(row, column, id, ..)| (*row, *column, *id));
    for (_, _, _, area, slots) in areas {
        for &stand in &slots.positions {
            if stand.xz().distance_squared(guest_position.xz()) > maximum_range * maximum_range {
                continue;
            }
            let Some(cell) = grid.cell_of(stand.xz()) else {
                continue;
            };
            let Some(cell_xy) = grid.cell_xy(cell) else {
                continue;
            };
            let cells = (animal_range / grid.cell_size_m).ceil() as i32;
            for y in -cells..=cells {
                for x in -cells..=cells {
                    let cx = cell_xy.x as i32 + x;
                    let cy = cell_xy.y as i32 + y;
                    if cx < 0 || cy < 0 || cx >= grid.width as i32 || cy >= grid.height as i32 {
                        continue;
                    }
                    for subject in
                        spatial_membership.members(world, cy as u32 * grid.width + cx as u32)
                    {
                        let Ok((subject_transform, visibility, species, member)) =
                            animals.get(subject)
                        else {
                            continue;
                        };
                        if member.root != world || *visibility == Visibility::Hidden {
                            continue;
                        }
                        let target = subject_transform.translation();
                        if target.xz().distance_squared(stand.xz()) > animal_range * animal_range
                            || !view_is_clear(
                                spatial_query,
                                parents,
                                stand + Vec3::Y * eye_height,
                                target,
                                guest,
                                area,
                                subject,
                            )
                        {
                            continue;
                        }
                        let weight = base_weight.saturating_mul(slots.priority);
                        let weight = if favourite == Some(species.species) {
                            scale_weight_q16(weight, preferred_animal_view_factor_q16)
                        } else {
                            weight
                        };
                        total_weight = total_weight.saturating_add(weight);
                        if total_weight != 0 && u64::from(rng.next_u32()) % total_weight < weight {
                            selected = Some(ViewingChoice {
                                area,
                                subject,
                                stand_position: stand,
                            });
                        }
                    }
                }
            }
        }
    }
    selected.map(|choice| (choice, total_weight))
}

fn scale_weight_q16(weight: u64, factor_q16: u32) -> u64 {
    ((u128::from(weight) * u128::from(factor_q16)) >> 16).min(u128::from(u64::MAX)) as u64
}

#[derive(Component, Default, PartialEq)]
pub(crate) struct ViewingGroundSlots {
    pub(crate) positions: Vec<Vec3>,
    priority: u64,
}

pub(super) fn project_authored_viewing_ground_slots(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_index: Single<Ref<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    changed_terrain: Query<(), Or<(Changed<TerrainChunk>, Changed<EditedTerrainSamples>)>>,
    mut removed_terrain_edits: RemovedComponents<EditedTerrainSamples>,
    mut areas: Query<
        (
            Entity,
            Ref<GlobalTransform>,
            Ref<Inspectable>,
            Option<&mut ViewingGroundSlots>,
        ),
        With<ViewingOpportunity>,
    >,
) {
    let edits_removed = removed_terrain_edits.read().count() != 0;
    let refresh_all = active_definitions.is_changed()
        || definitions.is_changed()
        || terrain_index.is_changed()
        || terrain_assets.is_changed()
        || edits_removed
        || !changed_terrain.is_empty();
    let Some(asset) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = asset.guest_viewing() else {
        return;
    };
    for (area, transform, inspectable, current) in &mut areas {
        if current.is_some() && !refresh_all && !transform.is_changed() && !inspectable.is_changed()
        {
            continue;
        }
        let next = asset
            .find_viewing_opportunity(inspectable.definition)
            .map_or_else(ViewingGroundSlots::default, |definition| {
                let positions = definition
                    .slots
                    .iter()
                    .map(|slot| transform.transform_point(Vec3::from_array(slot.local_position_m)))
                    .filter(|stand| {
                        valid_viewing_ground(
                            stand.xz(),
                            policy.minimum_open_cells,
                            policy.maximum_slope_cos,
                            policy.ground_distance_m,
                            &terrain_index,
                            &terrain_assets,
                            &terrain_chunks,
                        )
                    })
                    .collect::<Vec<_>>();
                ViewingGroundSlots {
                    positions,
                    priority: u64::from(definition.priority.max(1) as u16),
                }
            });
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(area).insert(next);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn valid_viewing_ground(
    center: Vec2,
    minimum_open_cells: u16,
    maximum_slope_cos: f32,
    spacing_m: f32,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> bool {
    let required = minimum_open_cells.max(1) as i32;
    let radius = ((required as f32).sqrt().ceil() as i32 - 1).max(0);
    let mut open = 0_u16;
    for z in -radius..=radius {
        for x in -radius..=radius {
            let position = center + Vec2::new(x as f32, z as f32) * spacing_m;
            let Some(chunk_entity) = terrain_chunk_at(terrain_index, position) else {
                continue;
            };
            let Ok((chunk, edited)) = terrain_chunks.get(chunk_entity) else {
                continue;
            };
            let Some(asset) = terrain_assets.get(&chunk.asset) else {
                continue;
            };
            if sample_terrain(chunk, asset, edited, position).is_some_and(|point| {
                point.water_height_m.is_none() && point.normal.y >= maximum_slope_cos
            }) {
                open += 1;
                if open >= minimum_open_cells.max(1) {
                    return true;
                }
            }
        }
    }
    false
}

fn belongs_to(mut entity: Entity, owner: Entity, parents: &Query<&ChildOf>) -> bool {
    for _ in 0..64 {
        if entity == owner {
            return true;
        }
        let Ok(parent) = parents.get(entity) else {
            return false;
        };
        entity = parent.parent();
    }
    false
}

fn view_is_clear(
    spatial_query: &SpatialQuery,
    parents: &Query<&ChildOf>,
    origin: Vec3,
    target: Vec3,
    guest: Entity,
    area: Entity,
    subject: Entity,
) -> bool {
    let offset = target - origin;
    let Ok(direction) = Dir3::new(offset) else {
        return true;
    };
    spatial_query
        .cast_ray_predicate(
            origin,
            direction,
            offset.length(),
            false,
            &SpatialQueryFilter::DEFAULT,
            &|hit| {
                !belongs_to(hit, guest, parents)
                    && !belongs_to(hit, area, parents)
                    && !belongs_to(hit, subject, parents)
            },
        )
        .is_none()
}

#[derive(Component)]
pub(crate) struct EntranceFailureReported;

pub(crate) fn cancel_failed_guest_destinations(
    mut failures: MessageReader<NavigationFailed>,
    mut guests: Query<&mut GuestNavigationRequest, With<Guest>>,
    arriving_guests: Query<
        (
            &GuestPhase,
            &WorldMember,
            &NavAgent,
            Has<EntranceFailureReported>,
        ),
        With<Guest>,
    >,
    entrances: Query<(&PersistentId, &WorldMember, &ZooEntrance)>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut navigate: MessageWriter<NavigateTo>,
    mut commands: Commands,
) {
    for failure in failures.read() {
        if let Ok(mut request) = guests.get_mut(failure.entity) {
            if request.0 != Some(failure.request_id) {
                continue;
            }
            request.0 = None;
            commands
                .entity(failure.entity)
                .remove::<(GuestDestination, Viewing, GuestViewingTarget)>();
            let Ok((phase, member, agent, reported)) = arriving_guests.get(failure.entity) else {
                continue;
            };
            if *phase != GuestPhase::Arriving {
                continue;
            }
            let Some((_, _, entrance)) = entrances
                .iter()
                .filter(|(_, entrance_member, _)| entrance_member.root == member.root)
                .min_by_key(|(id, _, _)| id.0)
            else {
                continue;
            };
            // Visiting guests can choose another activity after a failed route.
            // Arriving guests must reach the gate before admission unlocks that path.
            if !reported {
                warn!(guest = ?failure.entity, reason = ?failure.reason,
                    entrance = ?entrance.inside,
                    "arriving guest has no route to the entrance; retrying");
                commands
                    .entity(failure.entity)
                    .insert(EntranceFailureReported);
            }
            let Ok(mut generation) = navigation_generations.get_mut(failure.entity) else {
                continue;
            };
            let request_id = generation.advance();
            request.0 = Some(request_id);
            navigate.write(NavigateTo {
                entity: failure.entity,
                request_id,
                destination: entrance.inside,
                arrival_radius_m: agent.radius_m,
            });
        }
    }
}

pub(crate) fn begin_viewing(
    mut service_request_sequences: Query<&mut ServiceRequestSequence>,
    mut arrived: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut guests: Query<
        (
            Entity,
            &GuestDestination,
            &GuestPhase,
            &mut GuestNavigationRequest,
            Option<&GuestViewingTarget>,
            Option<&WorldMember>,
        ),
        (With<Guest>, Without<Viewing>),
    >,
    targets: Query<(&Visibility, Option<&ServiceFacility>), With<Inspectable>>,
    areas: Query<&WorldMember, With<ViewingOpportunity>>,
    animals: Query<
        &WorldMember,
        (
            With<Animal>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
        ),
    >,
    interruptions: Query<(), Or<(
        With<BehaviorTaskExecutionState>,
        With<openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet>,
        With<ServiceReservation>,
        With<openzt2_zoo_state::locomotion::locomotion_types::Destination>,
    )>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
) {
    // Both results may be queued before this consumer runs. Failure must win
    // before an arrival can hand off a service and discard its request identity.
    let failed_requests: bevy::platform::collections::HashSet<_> = failures
        .read()
        .map(|failure| (failure.entity, failure.request_id))
        .collect();
    for arrival in arrived.read() {
        if failed_requests.contains(&(arrival.entity, arrival.request_id)) {
            continue;
        }
        let Ok((guest, destination, phase, mut request, viewing_target, world)) =
            guests.get_mut(arrival.entity)
        else {
            continue;
        };
        // Transport owns Tour arrival, queueing and its eventual fare.
        if destination.purpose == GuestVisitPurpose::Tour {
            continue;
        }
        if *phase != GuestPhase::Visiting
            || request.0 != Some(arrival.request_id)
            || arrival
                .target
                .is_some_and(|target| target != destination.entity)
        {
            continue;
        }
        // Locomotion owns the arrival-radius decision. GlobalTransform may
        // still represent the previous rendered frame during this fixed tick.
        request.0 = None;
        if destination.purpose == GuestVisitPurpose::View && viewing_target.is_none() {
            commands
                .entity(guest)
                .remove::<(GuestDestination, Viewing, GuestViewingTarget)>();
            continue;
        }
        let Ok((visibility, facility)) = targets.get(destination.entity) else {
            commands
                .entity(guest)
                .remove::<(GuestDestination, Viewing, GuestViewingTarget)>();
            continue;
        };
        match reached_action(destination.purpose, *visibility != Visibility::Hidden) {
            ReachedAction::BeginViewing => {
                let subject = viewing_target
                    .expect("viewing destination has a selected subject")
                    .subject;
                let eligible = world.is_some_and(|world| {
                    areas
                        .get(destination.entity)
                        .is_ok_and(|area| area.root == world.root)
                        && animals
                            .get(subject)
                            .is_ok_and(|animal| animal.root == world.root)
                });
                let clock = world.and_then(|world| clocks.get(world.root).ok());
                let Some(clock) = clock.filter(|_| eligible && !interruptions.contains(guest))
                else {
                    commands
                        .entity(guest)
                        .remove::<(GuestDestination, GuestViewingTarget)>();
                    continue;
                };
                commands.entity(guest).insert(Viewing {
                    subject,
                    area: Some(destination.entity),
                    started_tick: clock.tick,
                    task_execution_id: None,
                });
                commands.entity(guest).remove::<GuestViewingTarget>();
            }
            ReachedAction::RequestService => {
                let Some(facility) = facility else {
                    commands.entity(guest).remove::<GuestDestination>();
                    continue;
                };
                let Some(sequence) = world
                    .and_then(|world| service_request_sequences.get_mut(world.root).ok())
                    .and_then(|mut sequence| sequence.allocate())
                else {
                    warn!(?guest, "service request sequence exhausted");
                    commands.entity(guest).remove::<GuestDestination>();
                    continue;
                };
                commands.entity(guest).insert(PendingServiceRequest {
                    sequence,
                    facility: destination.entity,
                    service: facility.definition,
                });
                commands.entity(guest).remove::<GuestDestination>();
            }
            ReachedAction::Invalid => {
                commands
                    .entity(guest)
                    .remove::<(GuestDestination, Viewing, GuestViewingTarget)>();
            }
            ReachedAction::CompleteExit => {}
        }
    }
}

pub(crate) fn advance_viewing(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    viewers: Query<
        (
            Entity,
            &GlobalTransform,
            &GuestArchetype,
            &GuestDestination,
            &Viewing,
        ),
        With<Guest>,
    >,
    viewing_areas: Query<&Visibility, With<ViewingOpportunity>>,
    animals: Query<(&GlobalTransform, &Visibility), With<Animal>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (guest, transform, archetype, destination, viewing) in &viewers {
        let Ok((subject_transform, subject_visibility)) = animals.get(viewing.subject) else {
            commands
                .entity(guest)
                .remove::<(Viewing, GuestDestination, GuestViewingTarget)>();
            continue;
        };
        let Some((asset, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let Some(policy) = asset.guest_viewing() else {
            continue;
        };
        let origin = transform.translation() + Vec3::Y * policy.guest_eye_height_m;
        let target = subject_transform.translation();
        let valid_area = viewing_areas
            .get(destination.entity)
            .is_ok_and(|visibility| *visibility != Visibility::Hidden);
        if *subject_visibility == Visibility::Hidden
            || !valid_area
            || origin.xz().distance_squared(target.xz()) > policy.ground_range_m.powi(2)
            || !view_is_clear(
                &spatial_query,
                &parents,
                origin,
                target,
                guest,
                destination.entity,
                viewing.subject,
            )
        {
            commands
                .entity(guest)
                .remove::<(Viewing, GuestDestination, GuestViewingTarget)>();
            continue;
        }
        // Patience only abandons this observation. Authored event delivery owns
        // viewing rewards; elapsed time cannot stand in for received events.
        if clock.tick.saturating_sub(viewing.started_tick)
            < u64::from(definition.patience_ticks.max(1))
        {
            continue;
        }
        commands
            .entity(guest)
            .remove::<(Viewing, GuestDestination, GuestViewingTarget)>();
    }
}

pub(crate) fn react_to_visible_litter(
    grid: Single<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    waste: Query<
        (Entity, &GlobalTransform, &WorldMember),
        Or<(Added<LooseLitterWaste>, Added<AnimalHabitatWaste>)>,
    >,
    guests: Query<&GuestArchetype, With<Guest>>,
    mut reactions: MessageWriter<GuestReaction>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (subject, transform, world) in &waste {
        let Some(cell) = grid.cell_of(transform.translation().xz()) else {
            continue;
        };
        for guest in spatial_membership.members(world.root, cell) {
            let Ok(archetype) = guests.get(guest) else {
                continue;
            };
            let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
                continue;
            };
            let Some(reaction) = find_reaction(definition, GuestMemoryKind::Litter) else {
                continue;
            };
            reactions.write(GuestReaction {
                guest,
                subject,
                kind: GuestMemoryKind::Litter,
                satisfaction_delta_permille: reaction.satisfaction_delta,
                education_delta_permille: reaction.education_delta,
            });
        }
    }
}

#[derive(Component)]
pub(super) struct ProjectedViewingOpportunity;

pub(super) fn project_viewing_opportunities(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    candidates: Query<(
        Entity,
        &Inspectable,
        Has<ViewingOpportunity>,
        Has<ProjectedViewingOpportunity>,
    )>,
    changed: Query<Entity, Changed<Inspectable>>,
    mut removed: RemovedComponents<Inspectable>,
    stale: Query<(), (With<ProjectedViewingOpportunity>, Without<Inspectable>)>,
    mut commands: Commands,
) {
    for entity in removed.read().filter(|&entity| stale.contains(entity)) {
        commands.entity(entity).remove::<(
            ViewingOpportunity,
            ViewingGroundSlots,
            ProjectedViewingOpportunity,
        )>();
    }
    let refresh_all = definitions.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if definitions.has_pending_assets() {
        return;
    }
    let affected = candidates
        .iter()
        .map(|(entity, ..)| entity)
        .take(if refresh_all { usize::MAX } else { 0 })
        .chain(
            changed
                .iter()
                .take(if refresh_all { 0 } else { usize::MAX }),
        );
    for (entity, inspectable, present, owned) in candidates.iter_many(affected).matched() {
        if definitions
            .find_viewing_opportunity(inspectable.definition)
            .is_some()
        {
            if !present {
                commands
                    .entity(entity)
                    .insert((ViewingOpportunity, ProjectedViewingOpportunity));
            }
        } else if owned {
            commands.entity(entity).remove::<(
                ViewingOpportunity,
                ViewingGroundSlots,
                ProjectedViewingOpportunity,
            )>();
        }
    }
}
