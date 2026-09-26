use super::guest_arrival_calculations::choose_guest_definition;
use super::guest_arrival_calculations::choose_guest_rarity;
use super::guest_definition_queries::find_guest_definition;
use super::guest_arrival_calculations::generation_delay_ticks;
use super::guest_arrival_calculations::guest_arrival_position;
use super::guest_definition_queries::guest_need_rows;
use super::guest_arrival_calculations::guest_spawn_probability;
use super::guest_arrival_calculations::random_unit;
use arrayvec::ArrayVec;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedDefinition;
use openzt2_game_data::AssetId;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::economy::facility_economy_types::Wallet;
use crate::plugins::economy::guest_admission_types::AdmissionPrice;
use crate::plugins::economy::guest_admission_types::ZooAdmissionsOpen;
use crate::plugins::economy::money_types::Money;
use crate::plugins::information::entity_selection_types::Inspectable;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::locomotion::locomotion_types::NavFlags;
use crate::plugins::locomotion::locomotion_types::NavigateTo;
use crate::plugins::locomotion::locomotion_types::NavigationRequestSequence;
use crate::plugins::person_name_selection_and_resolution::choose_generated_person_name_rows_from_authored_pool;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabPresentation;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;
use crate::plugins::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use super::guest_simulation_calculations::inclusive_u32;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestAmusement;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestArrivalFailed;
use super::guest_simulation_types::GuestArrivalFailure;
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

pub(super) fn initialize_guest_arrivals(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    seed: Res<ZooSeed>,
    roots: Query<&WorldRoot, With<WorldLoadCompleted>>,
    state: Option<Res<GuestArrivalState>>,
    mut commands: Commands,
    mut failed: MessageWriter<GuestArrivalFailed>,
) {
    if state.is_some() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for world in &roots {
        if definitions.guest_generation().is_none() {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::MissingPolicy,
            });
            continue;
        }
        let stable = u64::from_le_bytes(world.scenario.0[..8].try_into().unwrap()).max(1);
        commands.insert_resource(GuestArrivalState {
            next_tick: 0,
            rng: DeterministicRng::from_entity(*seed, PersistentId(stable), RngDomain::Guest),
        });
        break;
    }
}

pub(super) fn report_changed_guest_arrival_failures(
    mut failures: MessageReader<GuestArrivalFailed>,
    mut previous_failure: Local<Option<GuestArrivalFailure>>,
) {
    for failure in failures.read() {
        if previous_failure.as_ref() != Some(&failure.reason) {
            warn!(reason = ?failure.reason, "guest arrival is blocked");
            *previous_failure = Some(failure.reason);
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct GuestArrivalParams<'w, 's> {
    clock: Res<'w, ZooClock>,
    fame: Res<'w, Fame>,
    admission: Res<'w, AdmissionPrice>,
    admissions_open: Res<'w, ZooAdmissionsOpen>,
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    state: Option<ResMut<'w, GuestArrivalState>>,
    ids: ResMut<'w, PersistentIdAllocator>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    entrances: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static WorldMember,
            &'static ZooEntrance,
        ),
    >,
    guests: Query<'w, 's, &'static WorldMember, With<Guest>>,
    animals: Query<'w, 's, &'static SpeciesHandle>,
    seen_species: Local<'s, ArrayVec<AssetId, 512>>,
    commands: Commands<'w, 's>,
    navigate: MessageWriter<'w, NavigateTo>,
    navigation_request_sequence: ResMut<'w, NavigationRequestSequence>,
    failed: MessageWriter<'w, GuestArrivalFailed>,
    requested_arrivals: MessageReader<'w, 's, RequestGuestArrivals>,
}

pub(crate) fn spawn_arriving_guests(params: GuestArrivalParams) {
    let GuestArrivalParams {
        clock,
        fame,
        admission,
        admissions_open,
        definitions,
        active_definitions,
        state,
        mut ids,
        roots,
        entrances,
        guests,
        animals,
        mut seen_species,
        mut commands,
        mut navigate,
        mut navigation_request_sequence,
        mut failed,
        mut requested_arrivals,
    } = params;
    let Some(mut state) = state else {
        // The durable loaded-world root initializes this focused resource in
        // Update. A fixed tick may occur immediately after entering InGame
        // before that hydration system has run.
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let forced_count = requested_arrivals.read().fold(0_u32, |total, request| {
        total.saturating_add(u32::from(request.count))
    });
    if !admissions_open.0 && forced_count == 0 {
        return;
    }
    let scheduled_due = clock.tick >= state.next_tick;
    if !scheduled_due && forced_count == 0 {
        return;
    }
    let Ok(root) = roots.single() else {
        return;
    };
    let Some(policy) = definitions.guest_generation() else {
        failed.write(GuestArrivalFailed {
            reason: GuestArrivalFailure::MissingPolicy,
        });
        return;
    };
    let Some((_, member, entrance)) = entrances
        .iter()
        .filter(|(_, member, _)| member.root == root)
        .min_by_key(|(id, _, _)| id.0)
    else {
        failed.write(GuestArrivalFailed {
            reason: GuestArrivalFailure::MissingEntrance,
        });
        return;
    };
    if scheduled_due && forced_count == 0 && !policy.enabled {
        state.next_tick = clock.tick.saturating_add(1);
        return;
    }
    let interval = generation_delay_ticks(definitions, policy, &mut state.rng);
    if scheduled_due {
        state.next_tick = clock.tick.saturating_add(interval.max(1));
    }
    let population = guests.iter().filter(|guest| guest.root == root).count() as u32;
    seen_species.clear();
    for animal in &animals {
        if !seen_species.contains(&animal.species) && !seen_species.is_full() {
            seen_species.push(animal.species);
        }
    }
    seen_species.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let cap = policy.maximum_guests_base.saturating_add(
        policy
            .maximum_guests_per_half_star
            .saturating_mul(u32::from(fame.half_stars)),
    );
    let scheduled_count = if scheduled_due && policy.enabled && population < cap && {
        let probability = guest_spawn_probability(
            policy,
            fame.half_stars,
            admission.0 .0,
            seen_species.len() as u32,
        );
        random_unit(&mut state.rng) < probability
    } {
        1
    } else {
        0
    };
    let count = forced_count.saturating_add(scheduled_count);

    for _ in 0..count {
        let rarity = choose_guest_rarity(policy.rarity_thresholds, &mut state.rng);
        let Some(definition) = choose_guest_definition(definitions, rarity, &mut state.rng) else {
            break;
        };
        let guest_id = definition.id;
        if guest_need_rows(definition).is_none() {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::InvalidDefinition(guest_id),
            });
            continue;
        }
        let Some(prefab) = definitions
            .find_object(definition.object)
            .and_then(|object| definitions.scene(object.prefab))
        else {
            failed.write(GuestArrivalFailed {
                reason: GuestArrivalFailure::InvalidDefinition(guest_id),
            });
            continue;
        };
        let id = match ids.allocate(root) {
            Ok(id) => id,
            Err(reason) => {
                failed.write(GuestArrivalFailed {
                    reason: GuestArrivalFailure::PersistentId(reason),
                });
                break;
            }
        };
        let cash_low = definition.starting_cash_cents[0];
        let cash_high = definition.starting_cash_cents[1];
        let cash_width = i64::from(cash_high) - i64::from(cash_low) + 1;
        let cash = i64::from(cash_low) + i64::from(state.rng.next_u32()) % cash_width.max(1);
        let position = guest_arrival_position(policy, entrance.arrival, &mut state.rng);
        let favourite = (!seen_species.is_empty())
            .then(|| seen_species[state.rng.next_u32() as usize % seen_species.len()]);
        let request_id = navigation_request_sequence.next();
        let entity = commands
            .spawn((
                Guest,
                GuestNavigationRequest(Some(request_id)),
                GuestArchetype(guest_id),
                DefinitionId(definition.object),
                GuestFavouriteAnimal(favourite),
                id,
                *member,
                Wallet(Money(cash)),
                Inspectable {
                    definition: guest_id,
                },
                PrefabPresentation::new(prefab),
                Transform::from_translation(position),
                Visibility::Inherited,
                NavAgent {
                    radius_m: f32::from(definition.radius_cm) * 0.01,
                    max_speed_mps: definition.move_speed_mps,
                    acceleration_mps2: f32::MAX,
                    capabilities: NavFlags::GUEST,
                },
            ))
            .id();
        navigate.write(NavigateTo {
            entity,
            request_id,
            destination: entrance.inside,
            arrival_radius_m: f32::from(definition.radius_cm) * 0.01,
        });
    }
}

#[allow(clippy::type_complexity)]

pub(crate) fn initialize_spawned_guests(
    seed: Res<ZooSeed>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    fresh: Query<(Entity, &PersistentId, &GuestArchetype), (With<Guest>, Without<GuestRng>)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, id, archetype) in &fresh {
        let Some((asset, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let Some(needs) = guest_need_rows(definition) else {
            continue;
        };
        let name = choose_generated_person_name_rows_from_authored_pool(
            asset,
            AssetId(definition.name_pool.0),
            id.0,
        );
        let mut guest_rng = DeterministicRng::from_entity(*seed, *id, RngDomain::Guest);
        let mut initial_need_value = |need: &GuestNeedDefinition| {
            inclusive_u32(
                u32::from(need.initial_permille[0]),
                u32::from(need.initial_permille[1]),
                guest_rng.next_u32(),
            ) as u16
        };
        let initial_needs = needs.map(&mut initial_need_value);
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
                value: 1_000,
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
        if let Some(name) = name {
            entity_commands.insert(name);
        }
    }
}
