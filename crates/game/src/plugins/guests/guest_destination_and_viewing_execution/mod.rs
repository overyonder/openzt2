use super::guest_simulation_types::ViewingOpportunityClassified;
use avian3d::prelude::SpatialQuery;
use avian3d::prelude::SpatialQueryFilter;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_game_data::AssetId;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::animal_welfare::types::AnimalWelfare;
use crate::plugins::economy::facility_economy_types::ServiceFacility;
use crate::plugins::economy::service_types::ServiceRequest;
use crate::plugins::information::entity_selection_types::Inspectable;
use crate::plugins::locomotion::locomotion_types::Arrived;
use crate::plugins::locomotion::locomotion_types::NavigateTo;
use crate::plugins::locomotion::locomotion_types::NavigationFailed;
use crate::plugins::locomotion::locomotion_types::NavigationRequestSequence;
use crate::plugins::locomotion::locomotion_types::SpatialGrid;
use crate::plugins::maintenance::maintenance_types::AnimalHabitatWaste;
use crate::plugins::maintenance::maintenance_types::LooseLitterWaste;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use super::guest_simulation_calculations::reached_action;
use super::guest_simulation_calculations::ReachedAction;
use super::guest_simulation_types::Guest;
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

use crate::plugins::{
    behavior_task_execution_types::BehaviorTaskExecutionState,
    economy::service_types::ServiceReservation,
};

use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::find_reaction;
use super::guest_definition_queries::guest_need_rows;
use super::guest_definition_queries::need_value;
use super::guest_definition_queries::reconsider_threshold;
use super::guest_definition_queries::supports_purpose;

use crate::plugins::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::plugins::guests) fn choose_guest_destination(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<SpatialGrid>,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
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
            &GuestFavouriteAnimal,
            &mut GuestRng,
            &GuestPhase,
        ),
        (
            With<Guest>,
            Without<GuestDestination>,
            Without<ServiceReservation>,
            Without<BehaviorTaskExecutionState>,
            Without<Viewing>,
        ),
    >,
    inspectables: Query<(
        Entity,
        &GlobalTransform,
        &Inspectable,
        Option<&ServiceFacility>,
    )>,
    viewing_areas: Query<(Entity, &GlobalTransform, &Inspectable), With<ViewingOpportunity>>,
    animals: Query<(&GlobalTransform, &Visibility, &SpeciesHandle), With<Animal>>,
    mut commands: Commands,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
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
        favourite,
        mut rng,
        phase,
    ) in &mut guests
    {
        // Admission and departure own navigation until the guest is visiting.
        // Service reservations and authored tasks retain it until completion.
        if *phase != GuestPhase::Visiting {
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
            let cells = (radius_m / grid.cell_size_m).ceil() as i32;
            let purpose = weight.purpose;
            if purpose == GuestVisitPurpose::View {
                if let Some((choice, viewing_weight)) = choose_viewing_opportunity(
                    asset,
                    guest,
                    origin,
                    &grid,
                    &spatial_query,
                    &parents,
                    &terrain_index,
                    &terrain_assets,
                    &terrain_chunks,
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
            for y in -cells..=cells {
                for x in -cells..=cells {
                    let cx = cell_xy.x as i32 + x;
                    let cy = cell_xy.y as i32 + y;
                    if cx < 0 || cy < 0 || cx >= grid.width as i32 || cy >= grid.height as i32 {
                        continue;
                    }
                    let cell = cy as u32 * grid.width + cx as u32;
                    for candidate in grid.range(cell) {
                        if *candidate == guest {
                            continue;
                        }
                        let Ok((entity, transform, inspectable, facility)) =
                            inspectables.get(*candidate)
                        else {
                            continue;
                        };
                        if !supports_purpose(asset, inspectable.definition, purpose)
                            || facility.is_some_and(|facility| {
                                facility.capacity == 0 || facility.occupied >= facility.capacity
                            })
                            || transform.translation().xz().distance_squared(origin.xz())
                                > radius_m * radius_m
                        {
                            continue;
                        }
                        let candidate_weight = u64::from(base as u16)
                            * u64::from(pressure.saturating_sub(threshold).saturating_add(1));
                        total_weight = total_weight.saturating_add(candidate_weight);
                        if total_weight != 0
                            && u64::from(rng.0.next_u32()) % total_weight < candidate_weight
                        {
                            selected = Some((entity, transform.translation(), purpose));
                            viewing_target = None;
                        }
                    }
                }
            }
        }
        if let Some((entity, destination, purpose)) = selected {
            let request_id = navigation_request_sequence.next();
            let mut entity_commands = commands.entity(guest);
            entity_commands.insert((
                GuestDestination { entity, purpose },
                GuestNavigationRequest(Some(request_id)),
            ));
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
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    viewing_areas: &Query<(Entity, &GlobalTransform, &Inspectable), With<ViewingOpportunity>>,
    animals: &Query<(&GlobalTransform, &Visibility, &SpeciesHandle), With<Animal>>,
    favourite: Option<AssetId>,
    preferred_animal_view_factor_q16: u32,
    rng: &mut DeterministicRng,
    base_weight: u64,
) -> Option<(ViewingChoice, u64)> {
    let policy = asset.guest_viewing()?;
    let maximum_range = policy.ground_range_m;
    let animal_range = policy.ground_range_m;
    let eye_height = policy.guest_eye_height_m;
    let mut selected = None;
    let mut total_weight = 0_u64;

    let guest_cell = grid.cell_of(guest_position.xz())?;
    let guest_cell_xy = grid.cell_xy(guest_cell)?;
    let area_cells = (maximum_range / grid.cell_size_m).ceil() as i32;
    for area_y in -area_cells..=area_cells {
        for area_x in -area_cells..=area_cells {
            let column = guest_cell_xy.x as i32 + area_x;
            let row = guest_cell_xy.y as i32 + area_y;
            if column < 0 || row < 0 || column >= grid.width as i32 || row >= grid.height as i32 {
                continue;
            }
            let area_cell = row as u32 * grid.width + column as u32;
            for area_entity in grid.range(area_cell) {
                let Ok((area, transform, inspectable)) = viewing_areas.get(*area_entity) else {
                    continue;
                };
                let Some(definition) = asset.find_viewing_opportunity(inspectable.definition)
                else {
                    continue;
                };
                let priority = u64::from(definition.priority.max(1) as u16);
                for slot in &definition.slots {
                    let local = Vec3::new(
                        slot.local_position_m[0],
                        slot.local_position_m[1],
                        slot.local_position_m[2],
                    );
                    let stand = transform.transform_point(local);
                    if stand.xz().distance_squared(guest_position.xz())
                        > maximum_range * maximum_range
                        || !valid_viewing_ground(
                            stand.xz(),
                            policy.minimum_open_cells,
                            policy.maximum_slope_cos,
                            policy.ground_distance_m,
                            terrain_index,
                            terrain_assets,
                            terrain_chunks,
                        )
                    {
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
                            if cx < 0
                                || cy < 0
                                || cx >= grid.width as i32
                                || cy >= grid.height as i32
                            {
                                continue;
                            }
                            for subject in grid.range(cy as u32 * grid.width + cx as u32) {
                                let Ok((subject_transform, visibility, species)) =
                                    animals.get(*subject)
                                else {
                                    continue;
                                };
                                if *visibility == Visibility::Hidden {
                                    continue;
                                }
                                let target = subject_transform.translation();
                                if target.xz().distance_squared(stand.xz())
                                    > animal_range * animal_range
                                    || !view_is_clear(
                                        spatial_query,
                                        parents,
                                        stand + Vec3::Y * eye_height,
                                        target,
                                        guest,
                                        area,
                                        *subject,
                                    )
                                {
                                    continue;
                                }
                                let weight = base_weight.saturating_mul(priority);
                                let weight = if favourite == Some(species.species) {
                                    scale_weight_q16(weight, preferred_animal_view_factor_q16)
                                } else {
                                    weight
                                };
                                total_weight = total_weight.saturating_add(weight);
                                if total_weight != 0
                                    && u64::from(rng.next_u32()) % total_weight < weight
                                {
                                    selected = Some(ViewingChoice {
                                        area,
                                        subject: *subject,
                                        stand_position: stand,
                                    });
                                }
                            }
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

pub(in crate::plugins::guests) fn cancel_failed_guest_destinations(
    mut failures: MessageReader<NavigationFailed>,
    mut guests: Query<&mut GuestNavigationRequest, With<Guest>>,
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
        }
    }
}

pub(crate) fn begin_viewing(
    mut arrived: MessageReader<Arrived>,
    mut guests: Query<
        (
            Entity,
            &GuestDestination,
            &GuestPhase,
            &mut GuestNavigationRequest,
            Option<&GuestViewingTarget>,
        ),
        (With<Guest>, Without<Viewing>),
    >,
    targets: Query<(&Visibility, Option<&ServiceFacility>), With<Inspectable>>,
    mut commands: Commands,
    mut services: MessageWriter<ServiceRequest>,
) {
    for arrival in arrived.read() {
        let Ok((guest, destination, phase, mut request, viewing_target)) =
            guests.get_mut(arrival.entity)
        else {
            continue;
        };
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
                let subject = viewing_target.map_or(destination.entity, |target| target.subject);
                commands.entity(guest).insert(Viewing {
                    subject,
                    elapsed_ticks: 0,
                });
                commands.entity(guest).remove::<GuestViewingTarget>();
            }
            ReachedAction::RequestService => {
                let Some(facility) = facility else {
                    commands.entity(guest).remove::<GuestDestination>();
                    continue;
                };
                services.write(ServiceRequest {
                    customer: guest,
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

pub(in crate::plugins::guests) fn advance_viewing(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    spatial_query: SpatialQuery,
    parents: Query<&ChildOf>,
    mut viewers: Query<
        (
            Entity,
            &GlobalTransform,
            &GuestArchetype,
            &GuestDestination,
            &mut Viewing,
        ),
        With<Guest>,
    >,
    viewing_areas: Query<&Visibility, With<ViewingOpportunity>>,
    animals: Query<
        (
            &GlobalTransform,
            &SpeciesHandle,
            &AnimalWelfare,
            &Visibility,
        ),
        With<Animal>,
    >,
    mut commands: Commands,
    mut reactions: MessageWriter<GuestReaction>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (guest, transform, archetype, destination, mut viewing) in &mut viewers {
        let Ok((subject_transform, _, _, subject_visibility)) = animals.get(viewing.subject) else {
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
        viewing.elapsed_ticks = viewing.elapsed_ticks.saturating_add(1);
        if viewing.elapsed_ticks < u64::from(definition.patience_ticks.max(1)) {
            continue;
        }
        if let Some(reaction) = find_reaction(definition, GuestMemoryKind::AnimalView) {
            reactions.write(GuestReaction {
                guest,
                subject: viewing.subject,
                kind: GuestMemoryKind::AnimalView,
                satisfaction_delta_permille: reaction.satisfaction_delta,
                education_delta_permille: reaction.education_delta,
            });
        }
        commands
            .entity(guest)
            .remove::<(Viewing, GuestDestination, GuestViewingTarget)>();
    }
}

pub(in crate::plugins::guests) fn react_to_visible_litter(
    grid: Res<SpatialGrid>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    waste: Query<
        (Entity, &GlobalTransform),
        Or<(Added<LooseLitterWaste>, Added<AnimalHabitatWaste>)>,
    >,
    mut guests: Query<(&GlobalTransform, &GuestArchetype, &mut GuestRng), With<Guest>>,
    mut reactions: MessageWriter<GuestReaction>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (subject, transform) in &waste {
        let Some(cell) = grid.cell_of(transform.translation().xz()) else {
            continue;
        };
        for guest in grid.range(cell) {
            let Ok((_, archetype, _)) = guests.get_mut(*guest) else {
                continue;
            };
            let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
                continue;
            };
            let Some(reaction) = find_reaction(definition, GuestMemoryKind::Litter) else {
                continue;
            };
            reactions.write(GuestReaction {
                guest: *guest,
                subject,
                kind: GuestMemoryKind::Litter,
                satisfaction_delta_permille: reaction.satisfaction_delta,
                education_delta_permille: reaction.education_delta,
            });
        }
    }
}

pub(super) fn hydrate_viewing_opportunities(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    candidates: Query<(Entity, &Inspectable), Without<ViewingOpportunityClassified>>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, inspectable) in &candidates {
        let mut entity = commands.entity(entity);
        entity.insert(ViewingOpportunityClassified);
        if definitions
            .find_viewing_opportunity(inspectable.definition)
            .is_some()
        {
            entity.insert(ViewingOpportunity);
        }
    }
}
