use super::guest_arrival_calculations::{arrival_roll_passes, displaced_from_entrance_gate, generation_delay_ns, guest_population_cap, guest_spawn_probability};
use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::guest_need_rows;
use bevy::ecs::system::SystemParam;
use super::guest_arrival_population::ArrivalPopulation;
use super::guest_need_execution::GuestArchetypeTables;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedDefinition;
use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use super::guest_viewing_area_sensor::ViewingAreaSensors;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPaused;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::locomotion::locomotion_types::{EntranceNavigationConnection, NavAgent};
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_zoo_state::person_name_selection_and_resolution::select_authored_person_name;
use openzt2_zoo_state::progression::fame_types::Fame;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::{native_visual_random_stream::NativeVisualRandomStream, simulation_clock_types::ZooClock};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentIdAllocator, PersistentIdError};
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use super::guest_simulation_calculations::inclusive_u32;
use super::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestEntranceApproach;
use super::guest_simulation_types::GuestAmusement;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestArrivalFailed;
use super::guest_simulation_types::{GuestArrivalFailure, GuestReachedEntrance};
use super::guest_simulation_types::GuestArrivalState;
use super::guest_simulation_types::GuestDeparturePoints;
use super::guest_simulation_types::GuestDessert;
use super::guest_simulation_types::GuestEducation;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestFavouriteAnimal;
use super::guest_simulation_types::GuestGift;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestMemories;
use super::guest_simulation_types::GuestNavigationRequest;
use super::guest_simulation_types::GuestPhase;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestRng;
use super::guest_simulation_types::GuestSatisfaction;
use super::guest_simulation_types::GuestSocial;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingNeed;
use super::guest_simulation_types::RequestGuestArrivals;
use super::guest_simulation_types::VisitTime;
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;

pub(super) fn initialize_guest_arrivals(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    seed: Single<&ZooSeed>,
    roots: Query<
        (Entity, &WorldRoot, Has<GuestArrivalState>),
        (With<WorldLoadCompleted>, Without<GuestArrivalSchedule>),
    >,
    mut commands: Commands,
    mut failed: MessageWriter<GuestArrivalFailed>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if definitions.has_pending_assets() {
        return;
    }
    for (root, world, has_arrival_state) in &roots {
        let Some(policy) = definitions.guest_generation() else {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::MissingPolicy,
            });
            continue;
        };
        commands.entity(root).insert(GuestArrivalSchedule {
            probability_per_thousand: (policy.base_spawn_probability * 1000.0) as i32,
            population_cap: policy.maximum_guests_base,
            viewability_poll_at: 0,
            stagger_column: 0,
            stagger_row: 0,
        });
        if has_arrival_state {
            continue;
        }
        let stable = u64::from_le_bytes(world.scenario.0[..8].try_into().unwrap()).max(1);
        commands.entity(root).insert(GuestArrivalState {
            next_tick: 0,
            rng: DeterministicRng::from_entity(**seed, PersistentId(stable), RngDomain::Guest),
        });
    }
}

#[derive(Component)]
pub(super) struct GuestArrivalSchedule {
    probability_per_thousand: i32,
    population_cap: u32,
    viewability_poll_at: u64,
    stagger_column: u32,
    stagger_row: u32,
}

impl GuestArrivalSchedule {
    fn advance_stagger(&mut self, width: f32) -> Vec2 {
        if self.stagger_column as f32 >= width {
            self.stagger_column = 0;
            self.stagger_row = self.stagger_row.wrapping_add(1);
        }
        self.stagger_column += 1;
        Vec2::new(self.stagger_column as f32, self.stagger_row as f32)
    }
}

#[derive(Component, Default)]
pub(super) struct GuestArrivalDiagnostics(Option<GuestArrivalFailure>);

pub(super) fn project_guest_arrival_failures(
    mut failures: MessageReader<GuestArrivalFailed>,
    mut root: Single<&mut GuestArrivalDiagnostics, With<WorldRoot>>,
) {
    if let Some(failure) = failures.read().last() {
        root.reborrow()
            .map_unchanged(|status| &mut status.0)
            .set_if_neq(Some(failure.reason));
    }
}

pub(super) fn report_changed_guest_arrival_failures(
    roots: Query<&GuestArrivalDiagnostics, Changed<GuestArrivalDiagnostics>>,
) {
    for diagnostics in &roots {
        if let Some(reason) = diagnostics.0 {
            warn!(?reason, "guest arrival is blocked");
        }
    }
}

#[derive(Message)]
pub struct GuestViewabilityRefreshRequested {
    pub root: Entity,
}

pub(super) fn guest_arrivals_are_due(
    clock: Single<&ZooClock>,
    roots: Query<
        (
            &GuestArrivalState,
            &GuestArrivalSchedule,
            Option<&SimulationPaused>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut forced: MessageReader<RequestGuestArrivals>,
) -> bool {
    let requested = forced.read().count() != 0;
    let enabled = active
        .get(&assets)
        .and_then(|definitions| definitions.guest_generation())
        .is_some_and(|policy| policy.enabled);
    roots.single().is_ok_and(|(state, schedule, paused)| {
        requested
            || (enabled
                && paused.is_none_or(|paused| !paused.0)
                && (clock.tick >= state.next_tick || clock.tick >= schedule.viewability_poll_at))
    })
}

#[derive(SystemParam)]
pub(crate) struct GuestArrivalParams<'w, 's> {
    clock: Single<'w, 's, &'static ZooClock>,
    seed: Single<'w, 's, &'static ZooSeed>,
    admissions_open: Single<'w, 's, &'static ZooAdmissionsOpen>,
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    archetypes: Res<'w, GuestArchetypeTables>,
    random: ResMut<'w, NativeVisualRandomStream>,
    localization: Option<Res<'w, LocalizationPrecedenceIndex>>,
    localization_assets: Option<Res<'w, Assets<LocalizationAsset>>>,
    ids: Query<'w, 's, &'static mut PersistentIdAllocator>,
    roots: Query<
        'w,
        's,
        (
            Entity,
            &'static mut GuestArrivalState,
            &'static mut GuestArrivalSchedule,
            Option<&'static mut GuestFavouriteAnimalTrend>,
            Option<&'static Fame>,
            &'static ArrivalPopulation,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    entrances: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static WorldMember,
            &'static ZooEntrance,
            &'static GlobalTransform,
        ),
    >,
    grid: Single<'w, 's, &'static SpatialGrid>,
    viewing: ViewingAreaSensors<'w, 's>,
    pause: Query<'w, 's, &'static SimulationPaused>,
    viewing_refresh: MessageWriter<'w, GuestViewabilityRefreshRequested>,
    commands: Commands<'w, 's>,
    navigate: MessageWriter<'w, NavigateTo>,
    reached_entrance: MessageWriter<'w, GuestReachedEntrance>,
    failed: MessageWriter<'w, GuestArrivalFailed>,
    requested_arrivals: MessageReader<'w, 's, RequestGuestArrivals>,
}

pub(crate) fn spawn_arriving_guests(params: GuestArrivalParams) {
    let GuestArrivalParams {
        clock,
        seed,
        admissions_open,
        definitions,
        active_definitions,
        archetypes,
        mut random,
        localization,
        localization_assets,
        mut ids,
        mut roots,
        entrances,
        grid,
        viewing,
        pause,
        mut viewing_refresh,
        mut commands,
        mut navigate,
        mut reached_entrance,
        mut failed,
        mut requested_arrivals,
    } = params;
    let Ok((root, mut state, mut schedule, mut favourite_trend, fame, counts)) = roots.single_mut()
    else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let (Some(policy), Some(timestep)) = (
        definitions.guest_generation(),
        definitions.simulation_timestep(),
    ) else {
        failed.write(GuestArrivalFailed {
            reason: GuestArrivalFailure::MissingPolicy,
        });
        return;
    };
    if definitions.has_pending_assets() {
        return;
    }
    let forced_count = requested_arrivals.read().fold(0_u32, |total, request| {
        total.saturating_add(u32::from(request.count))
    });
    let automatic_on = policy.enabled && pause.get(root).map_or(true, |paused| !paused.0);
    let worklist = viewing.worklists.get(root).ok();
    let viewable =
        worklist.is_some_and(|worklist| worklist.is_viewable(viewing.has_intersections(worklist)));
    if automatic_on && clock.tick >= schedule.viewability_poll_at {
        let ticks = 15_000_000_000_u128.div_ceil(timestep.as_nanos());
        schedule.viewability_poll_at = clock
            .tick
            .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
        if !viewable && worklist.is_some_and(|worklist| worklist.has_viewable_candidates) {
            viewing_refresh.write(GuestViewabilityRefreshRequested { root });
        }
    }
    let scheduled_due = automatic_on && clock.tick >= state.next_tick;
    let automatic_attempt =
        scheduled_due && arrival_roll_passes(random.next_u15(), schedule.probability_per_thousand);
    if !scheduled_due && forced_count == 0 {
        return;
    }
    let anchor = entrances
        .iter()
        .filter(|(_, member, _, _)| member.root == root)
        .min_by_key(|(id, _, _, _)| id.0)
        .map(|(_, _, entrance, gate)| (entrance, gate));
    let localization = localization
        .as_deref()
        .zip(localization_assets.as_deref())
        .and_then(|(index, assets)| index.borrow_loaded_localization_view(assets));
    let half_stars = fame.map_or(0, |fame| fame.half_stars);

    let attempts = automatic_attempt
        .then_some(Some(true))
        .into_iter()
        .chain(scheduled_due.then_some(None))
        .chain((0..forced_count).map(|_| Some(false)));
    for attempt in attempts {
        let Some(scheduled) = attempt else {
            schedule.probability_per_thousand = guest_spawn_probability(policy, half_stars);
            schedule.population_cap = guest_population_cap(policy, half_stars);
            let delay = generation_delay_ns(policy.delay_ns, || random.next_u15());
            let ticks = u128::from(delay).div_ceil(timestep.as_nanos());
            state.next_tick = clock
                .tick
                .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
            continue;
        };
        let stagger = if policy.stagger {
            schedule.advance_stagger(policy.stagger_square_m)
        } else {
            Vec2::ZERO
        };
        if !admissions_open.0
            || grid.width == 0
            || grid.height == 0
            || !viewable
            || (scheduled && counts.guests >= schedule.population_cap)
        {
            continue;
        }

        let Some(definition) = archetypes
            .choose_archetype(policy.use_test_type, policy.rarity_thresholds, || {
                random.next_u15()
            })
            .and_then(|id| definitions.find_guest(id))
        else {
            continue;
        };
        let guest_id = definition.id;
        let Some(needs) = guest_need_rows(definition) else {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::MissingNeedRows(guest_id),
            });
            continue;
        };
        let Some(object) = definitions.find_object(definition.object) else {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::MissingObject {
                    guest: guest_id,
                    object: definition.object,
                },
            });
            continue;
        };
        let Some(prefab) = definitions.scene(object.prefab) else {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::MissingPrefabScene {
                    guest: guest_id,
                    prefab: object.prefab,
                },
            });
            continue;
        };
        let id = match ids
            .get_mut(root)
            .map_err(|_| PersistentIdError::WrongWorld)
            .and_then(|mut allocator| allocator.allocate())
        {
            Ok(id) => id,
            Err(reason) => {
                failed.write(GuestArrivalFailed {
                    reason: GuestArrivalFailure::PersistentId(reason),
                });
                continue;
            }
        };
        let mut initial_needs = [0; 8];
        for index in [0, 1, 2, 4, 6, 7, 3, 5] {
            let [minimum_wellness, maximum_wellness] = needs[index].initial_permille;
            let n = maximum_wellness.saturating_sub(minimum_wellness) / 10;
            let offset = if n == 0 {
                0
            } else {
                random.next_u15() % (n + 1)
            };
            initial_needs[index] = maximum_wellness - offset * 10;
        }
        let tour_need = definition.view_scoring.tour_need.zip(definitions.tour_scoring())
            .map(|(policy, scoring)| {
                let need = sample_source_stat(policy.start, || random.next_u15());
                openzt2_zoo_state::behavior_task_execution_types::running_actions::GuestTourDepartureGate {
                    need, threshold: scoring.need_threshold, policy, triggered: need >= policy.trigger,
                }
            });
        let show_need = definitions.guest_show_need(guest_id).and_then(|policy| {
            let from_q16 = |value: i32| value as f32 / 65_536.0;
            let trigger = from_q16(policy.trigger_q16?);
            let cessation = from_q16(policy.cessation_q16?);
            let [minimum, maximum] = policy.initial_q16.map(from_q16);
            let range = (minimum < maximum).then(|| (maximum - minimum) as u32);
            Some(
                openzt2_zoo_state::shows::running_show_types::GuestShowNeed::from_authored(
                    minimum,
                    range,
                    trigger,
                    cessation,
                    || random.next_u15(),
                ),
            )
        });
        let name = localization.as_ref().and_then(|localization| {
            let pool = localization.find_person_name_pool(definition.name_pool)?;
            let mut draw_name_index =
                |count: usize| (count > 0).then(|| (usize::from(random.next_u15()) % count) as u32);
            let first = draw_name_index(pool.first_names.len());
            let last = draw_name_index(pool.last_names.len());
            localization
                .render_person_name(definition.name_pool, first, last)
                .filter(|name| !name.is_empty())
                .map(Name::new)
        });
        let (position, rotation) = if policy.spawn_at_entrance {
            let Some((entrance, gate)) = anchor else {
                failed.write(GuestArrivalFailed {
                    reason: GuestArrivalFailure::MissingEntrance,
                });
                continue;
            };
            let offset = policy.arrival_offset_m.map(|value| -value);
            (
                displaced_from_entrance_gate(entrance.arrival, gate.rotation(), offset),
                gate.rotation() * Quat::from_rotation_y(std::f32::consts::PI),
            )
        } else {
            let [x, z] = policy.default_emitter_m;
            (
                Vec3::new(x - stagger.x, 0.0, -z + stagger.y),
                Quat::IDENTITY,
            )
        };
        let [cash_low, cash_high] = definition.starting_cash_cents.map(i64::from);
        let cash = if cash_high > cash_low {
            cash_low + i64::from(random.next_u15()) % (cash_high - cash_low + 1)
        } else {
            cash_low
        };
        let favourite = definitions.guest_favourite_animal_trend()
            .zip(favourite_trend.as_deref_mut())
            .and_then(|(policy, trend)| super::guest_favourite_animal_trend_execution::choose_favourite_animal_for_spawned_guest(
                policy,
                trend,
                definitions.animal_adoption_species_eligibility(),
                fame.and_then(|fame| fame.current_percent).unwrap_or(0.0),
                || random.next_u15(),
            ));
        let mut generation = NavigationRequestGeneration::default();
        let request_id = generation.advance();
        let entity = commands
            .spawn((
                Guest,
                GuestNavigationRequest(policy.spawn_at_entrance.then_some(request_id)),
                generation,
                GuestArchetype(guest_id),
                DefinitionId(definition.object),
                GuestFavouriteAnimal(favourite),
                id,
                WorldMember { root },
                Wallet(Money::from_cents(cash)),
                Inspectable {
                    definition: guest_id,
                },
                PrefabPresentation::new(prefab),
                Transform::from_translation(position).with_rotation(rotation),
                Visibility::Inherited,
                NavAgent {
                    radius_m: f32::from(definition.radius_cm) * 0.01,
                    max_speed_mps: definition.move_speed_mps,
                    acceleration_mps2: f32::MAX,
                    capabilities: NavFlags::GUEST,
                },
            ))
            .id();
        insert_guest_visit_state(
            &mut commands,
            entity,
            definition,
            initial_needs,
            DeterministicRng::from_entity(**seed, id, RngDomain::Guest),
        );
        if let Some(need) = tour_need {
            commands.entity(entity).insert(need);
        }
        if let Some(need) = show_need {
            commands.entity(entity).insert(need);
        }
        if let Some(name) = name {
            commands.entity(entity).insert(name);
        }
        reached_entrance.write(GuestReachedEntrance { guest: entity });
        if let Some((entrance, _)) = anchor.filter(|_| policy.spawn_at_entrance) {
            commands.entity(entity).insert(GuestEntranceApproach);
            if grid.cell_of(position.xz()).is_none() {
                commands
                    .entity(entity)
                    .insert(EntranceNavigationConnection {
                        request_id,
                        gate: entrance.arrival,
                        inbound: true,
                    });
            }
            navigate.write(NavigateTo {
                entity,
                request_id,
                destination: entrance.inside,
                arrival_radius_m: f32::from(definition.radius_cm) * 0.01,
            });
        }
    }
}

#[allow(clippy::type_complexity)]

pub(crate) fn initialize_spawned_guests(
    seed: Single<&ZooSeed>,
    localization: Option<Res<LocalizationPrecedenceIndex>>,
    localization_assets: Option<Res<Assets<LocalizationAsset>>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    fresh: Query<(Entity, &PersistentId, &GuestArchetype), (With<Guest>, Without<GuestRng>)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let localization = localization
        .as_deref()
        .zip(localization_assets.as_deref())
        .and_then(|(index, assets)| index.borrow_loaded_localization_view(assets));
    for (entity, id, archetype) in &fresh {
        let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let Some(needs) = guest_need_rows(definition) else {
            continue;
        };
        let name = localization.and_then(|localization| {
            select_authored_person_name(localization, [definition.name_pool], id.0)
        });
        let mut guest_rng = DeterministicRng::from_entity(**seed, *id, RngDomain::Guest);
        let mut initial_need_value = |need: &GuestNeedDefinition| {
            inclusive_u32(
                u32::from(need.initial_permille[0]),
                u32::from(need.initial_permille[1]),
                guest_rng.next_u32(),
            ) as u16
        };
        let initial_needs = needs.map(&mut initial_need_value);
        insert_guest_visit_state(&mut commands, entity, definition, initial_needs, guest_rng);
        let mut entity_commands = commands.entity(entity);
        if let Some(name) = name {
            entity_commands.insert_if_new(name);
        }
    }
}

fn sample_source_stat([minimum, maximum]: [f32; 2], draw: impl FnOnce() -> u16) -> f32 {
    if maximum > minimum {
        let width = ((maximum - minimum) as u32).saturating_add(1);
        minimum + (u32::from(draw()) % width) as f32
    } else {
        minimum
    }
}

fn insert_guest_visit_state(
    commands: &mut Commands,
    entity: Entity,
    definition: &openzt2_game_data::world_definitions::guest_simulation_definitions::GuestDefinition,
    initial_needs: [u16; 8],
    guest_rng: DeterministicRng,
) {
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert((
        GuestRng(guest_rng),
        GuestPhase::Arriving,
        GuestHunger {
            value: initial_needs[0],
            residual_q16: 0,
        },
        GuestThirst {
            value: initial_needs[1],
            residual_q16: 0,
        },
        GuestDessert {
            value: initial_needs[2],
            residual_q16: 0,
        },
        GuestGift {
            value: initial_needs[3],
            residual_q16: 0,
        },
        GuestEnergy {
            value: initial_needs[4],
            residual_q16: 0,
        },
        GuestRestroom {
            value: initial_needs[5],
            residual_q16: 0,
        },
        GuestSocial {
            value: initial_needs[6],
            residual_q16: 0,
        },
        GuestAmusement {
            value: initial_needs[7],
            residual_q16: 0,
        },
        definition
            .happiness_need
            .map_or(GuestSatisfaction(1_000, 0), |policy| {
                GuestSatisfaction::from_source_q16(policy.initial_q16)
            }),
        GuestEducation(0),
        GuestDeparturePoints(definition.initial_departure_points_q16),
        VisitTime::default(),
        GuestMemories::new(definition.memory.capacity),
    ));
    if let Some(viewing_need) = definition.viewing_need {
        entity_commands.insert(GuestViewingNeed::new(viewing_need.initial_q16));
    }
}
