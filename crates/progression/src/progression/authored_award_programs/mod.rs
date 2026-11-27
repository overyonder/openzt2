use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use mlua::{Function, Lua, MultiValue, Scope, Table, Value};
use openzt2_game_assets::{
    asset_source::{ArchiveOverlayChanged, AssetArchives},
    localization::{
        loaded_localization_queries::LoadedLocalizationView,
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    lua_script::{
        lua_script_asset_loading::LuaScriptAsset,
        lua_script_module_execution::{
            execute_lua_script_module_once_per_virtual_machine,
            install_lua_script_dependency_loading_functions,
        },
        lua_script_sources::LuaScriptSources,
    },
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{species::LifeStage, AssetId};
pub use openzt2_interface_state::information::award_presentation_types::{
    AwardGroupPresentation, AwardMilestonePresentation, AwardProgramEffect, AwardProgramOf,
    WorldAwardPrograms,
};
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    simulation_time::simulation_clock_types::{ZooClock, ZooMonth},
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        selected_world_identity::SelectedWorldIdentity,
        world_load_completion_marker::WorldLoadCompleted,
        world_membership_types::{DefinitionId, WorldMember, WorldMembers, WorldRoot},
    },
};
use openzt2_zoo_state::{
    economy::{
        account_transaction_types::{Account, TransactionKind, TransactionRequest},
        authored_transaction_amount::authored_currency_to_money,
        monthly_finance_types::{
            FinanceMetric, FinancePeriod, FinanceValue, MonthlyFinanceHistory,
        },
        zoo_cash_types::ZooCash,
    },
    progression::profile_scenario_variables::{
        PersistProfileScenarioVariables, ProfileScenarioVariables,
    },
};

use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{AnimalLifeStage, SpeciesHandle},
    donations::donation_payment_types::ZooDonationSummary,
    economy::{behavior_transaction_state::EconomyUsage, facility_economy_types::FacilityProfit},
    extinct_animals::{
        extinct_animal_observable_fact_types::FossilEducationProgress,
        fossil_collection_and_assembly_types::{FossilAssemblyCompleted, FossilCollections},
    },
    shows::show_schedule_types::ScheduledShowRating,
    transport_tours::{
        tour_scoring_types::StationRatingPermille, transport_circuit_types::CircuitMember,
        transport_topology_types::TransportStation,
    },
};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AwardWorldEntities<'w, 's> {
    donations: Query<'w, 's, &'static ZooDonationSummary>,
    show_ratings: Query<'w, 's, &'static ScheduledShowRating>,
    actors: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static DefinitionId>,
            Option<&'static SpeciesHandle>,
            Option<&'static AnimalLifeStage>,
        ),
        (Or<(With<DefinitionId>, With<SpeciesHandle>)>, Without<Dead>),
    >,
    stations: Query<
        'w,
        's,
        (
            &'static TransportStation,
            Option<&'static CircuitMember>,
            Option<&'static StationRatingPermille>,
        ),
    >,
    finances: Query<
        'w,
        's,
        (
            Option<&'static FacilityProfit>,
            Option<&'static EconomyUsage>,
        ),
    >,
    assembled_fossils: Query<'w, 's, (), With<FossilAssemblyCompleted>>,
    fossil_education: Query<'w, 's, &'static FossilEducationProgress>,
    traversable_areas: Query<'w, 's, &'static AwardTraversableArea>,
    collected_fossils: FossilCollections<'w, 's>,
    history: Query<'w, 's, &'static AwardDomainHistory, With<WorldRoot>>,
    definitions: Res<'w, WorldDefinitions>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    species: Option<Res<'w, SpeciesAssets>>,
    species_assets: Option<Res<'w, Assets<SpeciesAsset>>>,
}

use super::{
    award_and_progression_fact_types::{
        EarnedProgressionAward, ProgressionAwardGranted, ProgressionFactChanged,
    },
    profile_challenge_types::TotalAnimalsReleasedCount,
    zoo_rating_input_types::{GuestRatingFacts, LivingAnimalPopulation},
};

const AWARD_PROGRAM_DIRECTORY: &str = "scenario/scripts/awards";
const AWARD_GROUPS_PER_PASS: usize = 2;

#[derive(Component, Default, PartialEq, Eq)]
pub struct AuthoredAwardPoints(pub u32);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredAwardName(pub AssetId);

#[derive(Component, Default)]
pub struct AwardGlobalVariables(pub BTreeMap<AssetId, String>);

/// A challenge or result panel currently owns the award presentation slot.
#[derive(Component)]
pub struct AwardProgramSuspended;

/// Lifetime award inputs credited once by the domain committing each outcome.
#[derive(Component, Default)]
pub struct AwardDomainHistory {
    pub released_species: BTreeMap<AssetId, String>,
    pub extinct_released_species: BTreeSet<AssetId>,
    pub dinosaur_tranquilizations: u32,
    pub perfect_training_sessions: u32,
    pub cured_diseases: u32,
    pub super_clones: u32,
}

/// Cells reachable by an animal, grouped by their authored biome.
#[derive(Component, Default)]
pub struct AwardTraversableArea {
    pub total_cells: u32,
    pub biome_cells: BTreeMap<AssetId, u32>,
}

#[derive(Message)]
pub struct RecordAwardOutcome {
    pub root: Entity,
    pub outcome: AwardHistoryOutcome,
}

pub enum AwardHistoryOutcome {
    Released {
        species: AssetId,
        name: String,
        extinct: bool,
    },
    DinosaurTranquilized,
    PerfectTraining,
    DiseaseCured,
    SuperClone,
}

pub(super) fn record_award_outcomes(
    mut outcomes: MessageReader<RecordAwardOutcome>,
    mut histories: Query<&mut AwardDomainHistory, With<WorldRoot>>,
) {
    for entry in outcomes.read() {
        let Ok(mut history) = histories.get_mut(entry.root) else {
            continue;
        };
        match &entry.outcome {
            AwardHistoryOutcome::Released {
                species,
                name,
                extinct,
            } => {
                history
                    .released_species
                    .entry(*species)
                    .or_insert_with(|| name.clone());
                if *extinct {
                    history.extinct_released_species.insert(*species);
                }
            }
            AwardHistoryOutcome::DinosaurTranquilized => {
                history.dinosaur_tranquilizations =
                    history.dinosaur_tranquilizations.saturating_add(1);
            }
            AwardHistoryOutcome::PerfectTraining => {
                history.perfect_training_sessions =
                    history.perfect_training_sessions.saturating_add(1);
            }
            AwardHistoryOutcome::DiseaseCured => {
                history.cured_diseases = history.cured_diseases.saturating_add(1);
            }
            AwardHistoryOutcome::SuperClone => {
                history.super_clones = history.super_clones.saturating_add(1);
            }
        }
    }
}

#[derive(Component, Default)]
pub(super) struct AwardProgramCursor(usize);

#[derive(Message)]
pub(super) struct ShowAwardPanelAfterFame(Entity);

pub(super) fn publish_award_panels_after_fame(
    mut pending: MessageReader<ShowAwardPanelAfterFame>,
    mut panels: MessageWriter<AwardProgramEffect>,
) {
    for request in pending.read() {
        panels.write(AwardProgramEffect::ShowAwardPanel { root: request.0 });
    }
}

#[derive(Component)]
pub(super) struct AwardProgram {
    name: String,
    path: String,
    script: Handle<LuaScriptAsset>,
    lua: Option<Lua>,
    modules: BTreeSet<AssetId>,
}

pub(super) fn load_winning_award_programs(
    mut commands: Commands,
    archives: Res<AssetArchives>,
    sources: Res<LuaScriptSources>,
    mut overlay: MessageReader<ArchiveOverlayChanged>,
    mut asset_events: MessageReader<AssetEvent<LuaScriptAsset>>,
    roots: Query<(Entity, Ref<WorldLoadCompleted>, Option<&WorldAwardPrograms>), With<WorldRoot>>,
    mut programs: Query<(Entity, &mut AwardProgram)>,
) {
    let source_changed = overlay.read().any(|event| {
        event.changes.iter().any(|change| {
            change
                .normalized_asset_path
                .starts_with(AWARD_PROGRAM_DIRECTORY)
                || change.normalized_asset_path.starts_with("scenario/scripts")
        })
    });
    overlay.clear();
    let script_changes: BTreeSet<_> = asset_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
            _ => None,
        })
        .collect();
    let mut winners = None;
    for (root, loaded, current) in &roots {
        if source_changed || loaded.is_added() {
            let winners = winners.get_or_insert_with(|| {
                archives
                    .source_paths_under(Path::new(AWARD_PROGRAM_DIRECTORY))
                    .into_iter()
                    .filter(|path| {
                        path.extension().is_some_and(|ext| {
                            ext.eq_ignore_ascii_case("lua") || ext.eq_ignore_ascii_case("bin")
                        })
                    })
                    .filter_map(|path| {
                        let name = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
                        if name.starts_with("template_") {
                            return None;
                        }
                        let (path, script) =
                            sources.load("", &path.with_extension("lua").to_string_lossy())?;
                        Some((name, (path, script)))
                    })
                    .collect::<BTreeMap<_, _>>()
            });
            let mut ordered = BTreeMap::new();
            let mut current =
                programs.iter_many_mut(current.into_iter().flat_map(RelationshipTarget::iter));
            while let Some(program) = current.fetch_next() {
                let Ok((entity, mut program)) = program else {
                    continue;
                };
                if let Some((path, script)) = winners.get(&program.name) {
                    ordered.insert(program.name.clone(), entity);
                    if source_changed || script != &program.script {
                        program.path.clone_from(path);
                        program.script = script.clone();
                        program.lua = None;
                        program.modules.clear();
                    }
                } else {
                    commands.entity(entity).try_despawn();
                }
            }
            for (name, (path, script)) in winners {
                ordered.entry(name.clone()).or_insert_with(|| {
                    commands
                        .spawn((
                            AwardGroupPresentation::default(),
                            AwardProgram {
                                name: name.clone(),
                                path: path.clone(),
                                script: script.clone(),
                                lua: None,
                                modules: BTreeSet::new(),
                            },
                        ))
                        .id()
                });
            }
            commands
                .entity(root)
                .replace_related::<AwardProgramOf>(&ordered.into_values().collect::<Vec<_>>());
        } else if !script_changes.is_empty() {
            let mut current =
                programs.iter_many_mut(current.into_iter().flat_map(RelationshipTarget::iter));
            while let Some(program) = current.fetch_next() {
                if let Ok((_, mut program)) = program {
                    if script_changes.contains(&program.script.id()) {
                        program.lua = None;
                        program.modules.clear();
                    }
                }
            }
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AwardEffects<'w> {
    notifications: Option<
        MessageWriter<'w, openzt2_interface_state::ui::notification_contracts::ShowNotification>,
    >,
    display: MessageWriter<'w, AwardProgramEffect>,
    display_after_fame: MessageWriter<'w, ShowAwardPanelAfterFame>,
    granted: MessageWriter<'w, ProgressionAwardGranted>,
    changed: MessageWriter<'w, ProgressionFactChanged>,
    transactions: Option<MessageWriter<'w, TransactionRequest>>,
    persist: Option<MessageWriter<'w, PersistProfileScenarioVariables>>,
    profile: Option<ResMut<'w, ProfileScenarioVariables>>,
}

pub(super) fn process_authored_award_programs(
    mut roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &ZooClock,
            Option<&ZooMonth>,
            &ZooCash,
            Option<&MonthlyFinanceHistory>,
            &GuestRatingFacts,
            &LivingAnimalPopulation,
            &TotalAnimalsReleasedCount,
            &mut PersistentIdAllocator,
            &mut AuthoredAwardPoints,
            &mut AwardGlobalVariables,
            &mut AwardProgramCursor,
            &WorldAwardPrograms,
            Option<&WorldMembers>,
        ),
        (With<WorldLoadCompleted>, Without<AwardProgramSuspended>),
    >,
    mut programs: Query<(&mut AwardProgram, &mut AwardGroupPresentation)>,
    earned: Query<(&WorldMember, &EarnedProgressionAward)>,
    sources: Res<LuaScriptSources>,
    scripts: Res<Assets<LuaScriptAsset>>,
    localization: Option<Res<LocalizationPrecedenceIndex>>,
    locale_assets: Option<Res<Assets<LocalizationAsset>>>,
    facts: AwardWorldEntities,
    mut effects: AwardEffects,
    mut commands: Commands,
) {
    let localization = localization
        .as_deref()
        .zip(locale_assets.as_deref())
        .and_then(|(locale, assets)| locale.borrow_loaded_localization_view(assets));
    for (
        root,
        identity,
        clock,
        month,
        cash,
        history,
        guests,
        animals,
        released,
        mut ids,
        mut points,
        mut globals,
        mut cursor,
        groups,
        members,
    ) in &mut roots
    {
        let count = groups.len();
        if count == 0 {
            continue;
        }
        let start = cursor.0 % count;
        let length = AWARD_GROUPS_PER_PASS.min(count - start);
        cursor.0 = (start + length) % count;
        let mut earned_flags = earned
            .iter()
            .filter_map(|(member, award)| (member.root == root).then_some(award.definition))
            .collect::<BTreeSet<_>>();
        for group in groups.iter().skip(start).take(length) {
            let Ok((mut program, mut presentation)) = programs.get_mut(group) else {
                continue;
            };
            if !scripts.contains(program.script.id()) {
                continue;
            }
            let host = AwardFacts {
                root,
                mode: identity.mode,
                cash_dollars: cash.0.dollars(),
                total_visitors: history
                    .filter(|history| history.has_initialized_month())
                    .map_or(0, |history| {
                        match history.finance_metric_value(
                            FinanceMetric::TotalUsers,
                            FinancePeriod::Lifetime,
                        ) {
                            FinanceValue::Users(users) => users,
                            FinanceValue::Money(_) => 0,
                        }
                    }),
                guests: guests.count,
                animals: animals.animals,
                released: released.0,
                month: month.map_or(0, |month| month.0),
                globals: &globals,
                entities: &facts,
                members,
                profile: effects.profile.as_deref(),
                localization,
            };
            let next = match program.evaluate(&sources, &scripts, &host, &earned_flags) {
                Ok(next) => next,
                Err(error) => {
                    debug!(program = %program.name, %error, "award script failed");
                    continue;
                }
            };
            let mut apply_fame_before_panel = false;
            for command in next {
                match command {
                    AwardEffect::Document { path, persistent } => {
                        presentation
                            .reborrow()
                            .map_unchanged(|presentation| &mut presentation.document)
                            .set_if_neq(path);
                        presentation
                            .reborrow()
                            .map_unchanged(|presentation| &mut presentation.persistent)
                            .set_if_neq(persistent);
                    }
                    AwardEffect::Progress { index, value } => {
                        if presentation
                            .milestones
                            .get(&index)
                            .is_none_or(|milestone| milestone.progress != value)
                        {
                            presentation.milestones.entry(index).or_default().progress = value;
                        }
                    }
                    AwardEffect::Received(index) => {
                        if presentation
                            .milestones
                            .get(&index)
                            .is_none_or(|milestone| !milestone.received)
                        {
                            presentation.milestones.entry(index).or_default().received = true;
                        }
                    }
                    AwardEffect::Earned {
                        definition,
                        index,
                        name,
                    } => {
                        if !earned_flags.contains(&definition) {
                            let Ok(id) = ids.allocate() else { continue };
                            let award = commands
                                .spawn((
                                    id,
                                    WorldMember { root },
                                    EarnedProgressionAward {
                                        definition,
                                        earned_tick: clock.tick,
                                    },
                                    AuthoredAwardName(name),
                                ))
                                .id();
                            earned_flags.insert(definition);
                            effects
                                .granted
                                .write(ProgressionAwardGranted { award, definition });
                            effects
                                .changed
                                .write(ProgressionFactChanged::Award(definition));
                        }
                        if presentation
                            .milestones
                            .get(&index)
                            .is_none_or(|milestone| !milestone.received)
                        {
                            presentation.milestones.entry(index).or_default().received = true;
                        }
                    }
                    AwardEffect::AwardPoints(delta) => {
                        let next = points.0.saturating_add_signed(delta);
                        if points.0 != next {
                            points.0 = next;
                        }
                    }
                    AwardEffect::WorldVariable(key, value) => {
                        if let Some(value) = value {
                            if globals.0.get(&key) != Some(&value) {
                                globals.0.insert(key, value);
                            }
                        } else if globals.0.contains_key(&key) {
                            globals.0.remove(&key);
                        }
                    }
                    AwardEffect::UserVariable(key, value) => {
                        if let Some(profile) = effects.profile.as_mut() {
                            let _ = profile.set(key, value.as_deref());
                        }
                    }
                    AwardEffect::SaveOptions => {
                        if let Some(writer) = effects.persist.as_mut() {
                            writer.write(PersistProfileScenarioVariables);
                        }
                    }
                    AwardEffect::RefreshFame => apply_fame_before_panel = true,
                    AwardEffect::ShowPanel if apply_fame_before_panel => {
                        debug!(program = %program.name, "award program shows the award panel");
                        effects
                            .display_after_fame
                            .write(ShowAwardPanelAfterFame(root));
                    }
                    AwardEffect::ShowPanel => {
                        debug!(program = %program.name, "award program shows the award panel");
                        effects
                            .display
                            .write(AwardProgramEffect::ShowAwardPanel { root });
                    }
                    AwardEffect::Notify(text) => {
                        if let Some(writer) = effects.notifications.as_mut() {
                            let operation = commands
                                .spawn((WorldMember { root }, AwardNotification))
                                .id();
                            writer.write(openzt2_interface_state::ui::notification_contracts::ShowNotification {
                                operation,
                                message: openzt2_interface_state::ui::notification_contracts::NotificationMessage::Text(text),
                            });
                        }
                    }
                    AwardEffect::Crate(definition) => {
                        effects
                            .display
                            .write(AwardProgramEffect::PlaceCratedObject { root, definition });
                    }
                    AwardEffect::ChallengeWon(arguments) => {
                        effects
                            .display
                            .write(AwardProgramEffect::ShowChallengeWon { root, arguments });
                    }
                    AwardEffect::ReplaceWalls(definition) => {
                        effects
                            .display
                            .write(AwardProgramEffect::ReplaceZooWalls { root, definition });
                    }
                    AwardEffect::Ambient(actor) => {
                        effects
                            .display
                            .write(AwardProgramEffect::AddAmbientSound { root, cue: actor });
                    }
                    AwardEffect::Cash(dollars) => {
                        if let Some((writer, amount)) = effects
                            .transactions
                            .as_mut()
                            .zip(authored_currency_to_money(dollars))
                        {
                            let operation = commands.spawn(WorldMember { root }).id();
                            commands.entity(operation).insert(AwardCashPayment);
                            writer.write(TransactionRequest {
                                domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
                                operation,
                                debit: Account::External,
                                credit: Account::Zoo,
                                amount,
                                kind: TransactionKind::Reward,
                                subject: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

#[derive(Component)]
pub(super) struct AwardNotification;

pub(super) fn retire_resolved_award_notifications(
    mut presented: Option<
        MessageReader<openzt2_interface_state::ui::notification_contracts::NotificationPresented>,
    >,
    mut rejected: Option<
        MessageReader<openzt2_interface_state::ui::notification_contracts::NotificationRejected>,
    >,
    notifications: Query<(), With<AwardNotification>>,
    mut commands: Commands,
) {
    for operation in presented
        .as_mut()
        .into_iter()
        .flat_map(|reader| reader.read().map(|message| message.operation))
        .chain(
            rejected
                .as_mut()
                .into_iter()
                .flat_map(|reader| reader.read().map(|message| message.operation)),
        )
    {
        if notifications.contains(operation) {
            commands.entity(operation).try_despawn();
        }
    }
}

#[derive(Component)]
pub(super) struct AwardCashPayment;

pub(super) fn retire_settled_award_cash_payments(
    mut completed: Option<
        MessageReader<openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted>,
    >,
    mut rejected: Option<
        MessageReader<openzt2_zoo_state::economy::account_transaction_types::TransactionRejected>,
    >,
    pending: Query<(), With<AwardCashPayment>>,
    mut commands: Commands,
) {
    for operation in completed
        .as_mut()
        .into_iter()
        .flat_map(|reader| reader.read().map(|message| message.operation))
        .chain(
            rejected
                .as_mut()
                .into_iter()
                .flat_map(|reader| reader.read().map(|message| message.operation)),
        )
    {
        if pending.contains(operation) {
            commands.entity(operation).try_despawn();
        }
    }
}

struct AwardFacts<'a, 'w, 's> {
    root: Entity,
    mode: WorldSessionMode,
    cash_dollars: f32,
    total_visitors: i64,
    guests: u32,
    animals: u32,
    released: u32,
    month: u32,
    globals: &'a AwardGlobalVariables,
    entities: &'a AwardWorldEntities<'w, 's>,
    members: Option<&'a WorldMembers>,
    profile: Option<&'a ProfileScenarioVariables>,
    localization: Option<LoadedLocalizationView<'a>>,
}

impl AwardFacts<'_, '_, '_> {
    fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.members.into_iter().flat_map(RelationshipTarget::iter)
    }

    fn count_type(&self, authored_type: &str) -> usize {
        let requested = AssetId::from_key(&authored_type.to_ascii_lowercase());
        let juvenile_type = authored_type
            .to_ascii_lowercase()
            .strip_suffix("_young")
            .map(AssetId::from_key);
        let definitions = self.entities.definitions.get(&self.entities.assets);
        self.entities
            .actors
            .iter_many(self.entities())
            .flatten()
            .filter(|(_, definition, species, stage)| {
                species.is_some_and(|species| {
                    species.species == requested
                        || (juvenile_type == Some(species.species)
                            && stage.is_some_and(|stage| {
                                matches!(stage.0, LifeStage::Juvenile | LifeStage::Young)
                            }))
                }) || definition.is_some_and(|definition| {
                    definition.0 == requested
                        || definitions
                            .and_then(|view| view.find_object(definition.0))
                            .is_some_and(|record| {
                                record.behavior_type_identifiers.contains(&requested)
                            })
                })
            })
            .count()
    }

    fn authored_float(&self, definition: AssetId, key: AssetId) -> Option<f32> {
        self.entities
            .definitions
            .get(&self.entities.assets)?
            .find_object(definition)?
            .authored_runtime
            .numeric_behavior_properties
            .iter()
            .find_map(|&(name, value)| (name == key).then_some(value))
    }

    fn station_totals(&self, authored_type: &str) -> (f64, i64, f32) {
        use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
        let Some(definitions) = self.entities.definitions.get(&self.entities.assets) else {
            return (0.0, 0, 0.0);
        };
        let requested = AssetId::from_key(&authored_type.to_ascii_lowercase());
        let mut circuits = bevy::ecs::entity::EntityHashSet::default();
        let (mut income, mut users, mut stars) = (0.0, 0i64, 0.0f32);
        for (station, circuit, rating) in
            self.entities.stations.iter_many(self.entities()).flatten()
        {
            let Some(station) = definitions.find_station(station.definition) else {
                continue;
            };
            let authored_group = match station.kind {
                TransportationTrackKind::Ground => "groundstation",
                TransportationTrackKind::Sky => "skystation",
            };
            if station.id != requested
                && station.object != requested
                && !authored_group.eq_ignore_ascii_case(authored_type)
            {
                continue;
            }
            if let Some(rating) = rating {
                stars = stars.max(f32::from(rating.0) / 200.0);
            }
            if let Some(circuit) = circuit.filter(|circuit| circuits.insert(circuit.0)) {
                if let Ok((profit, usage)) = self.entities.finances.get(circuit.0) {
                    income += profit.map_or(0.0, |profit| f64::from(profit.total.dollars()));
                    users =
                        users.saturating_add(usage.map_or(0, |usage| i64::from(usage.counts()[1])));
                }
            }
        }
        (income, users, stars)
    }

    fn fossils_by_level(&self, sonar_level: u8, ready: bool) -> usize {
        let Some(definitions) = self.entities.definitions.get(&self.entities.assets) else {
            return 0;
        };
        let sets: BTreeSet<_> = definitions.fossil_pieces().map(|piece| piece.set).collect();
        sets.into_iter()
            .filter_map(|set| definitions.find_fossil_set(set))
            .filter(|set| {
                set.discovery_level == sonar_level
                    && (!ready
                        || set.pieces.iter().all(|&piece| {
                            self.entities.collected_fossils.contains(self.root, piece)
                        }))
            })
            .count()
    }
}

enum AwardEffect {
    Document {
        path: String,
        persistent: bool,
    },
    Progress {
        index: usize,
        value: f32,
    },
    Received(usize),
    Earned {
        definition: AssetId,
        index: usize,
        name: AssetId,
    },
    AwardPoints(i32),
    WorldVariable(AssetId, Option<String>),
    UserVariable(AssetId, Option<String>),
    SaveOptions,
    RefreshFame,
    ShowPanel,
    Notify(String),
    Crate(AssetId),
    ChallengeWon(Vec<String>),
    ReplaceWalls(AssetId),
    Ambient(AssetId),
    Cash(f64),
}

impl AwardProgram {
    fn evaluate(
        &mut self,
        sources: &LuaScriptSources,
        scripts: &Assets<LuaScriptAsset>,
        facts: &AwardFacts<'_, '_, '_>,
        earned: &BTreeSet<AssetId>,
    ) -> mlua::Result<Vec<AwardEffect>> {
        let initial_load = self.lua.is_none();
        let lua = self.lua.get_or_insert_with(Lua::new);
        let modules = RefCell::new(std::mem::take(&mut self.modules));
        let stack = RefCell::new(Vec::new());
        let effects = RefCell::new(Vec::new());
        let result = lua.scope(|scope| {
            install_lua_script_dependency_loading_functions(
                lua, scope, scripts, sources, &modules, &stack,
            )?;
            install_award_world_bindings(lua, scope, facts, &effects)?;
            execute_lua_script_module_once_per_virtual_machine(
                lua,
                &self.path,
                &self.script,
                scripts,
                &modules,
                &stack,
            )?;
            stack.borrow_mut().push(self.path.clone());
            install_award_world_bindings(lua, scope, facts, &effects)?;
            install_award_group_bindings(lua, scope, facts.mode, earned, &effects)?;
            let globals = lua.globals();
            let component = lua.create_table()?;
            if initial_load {
                if let Some(init) = globals.get::<Option<Function>>("initaward")? {
                    init.call::<()>(component.clone())?;
                }
            }
            if let Some(process) = globals.get::<Option<Function>>("processaward")? {
                process.call::<()>(component)?;
            }
            Ok(())
        });
        if result.is_ok() {
            self.modules = modules.into_inner();
        } else {
            self.lua = None;
            self.modules.clear();
        }
        result.map(|()| effects.into_inner())
    }
}

fn install_award_group_bindings<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    mode: WorldSessionMode,
    earned: &'env BTreeSet<AssetId>,
    effects: &'env RefCell<Vec<AwardEffect>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(
        "create_award_group_graphics",
        scope.create_function(move |_, group: Table| {
            if let Some(document) = group.get::<Option<String>>("group_xml")? {
                let persistent = group
                    .get::<Option<bool>>("persistent_awards")?
                    .unwrap_or(false);
                effects.borrow_mut().push(AwardEffect::Document {
                    path: document,
                    persistent,
                });
            }
            Ok(())
        })?,
    )?;
    globals.set(
        "process_award_group",
        scope.create_function(move |lua, (_, group): (Value, Table)| {
            if group
                .get::<Option<bool>>("is_challenge_only")?
                .unwrap_or(false)
                && mode != WorldSessionMode::Challenge
            {
                return Ok(());
            }
            let whole_group = group
                .get::<Option<bool>>("process_whole_group")?
                .unwrap_or(false);
            let test = group.get::<Function>("test_func")?;
            let mut complete = true;
            let mut recently_earned = false;
            for index in 1..=group.raw_len() {
                let milestone = group.get::<Table>(index)?;
                let definition = AssetId::from_key(&milestone.get::<String>("flag_name")?);
                if earned.contains(&definition) {
                    effects.borrow_mut().push(AwardEffect::Received(index));
                    continue;
                }
                let result = test.call::<Value>((group.clone(), index))?;
                let satisfied = !matches!(result, Value::Nil | Value::Boolean(false));
                if satisfied {
                    recently_earned = true;
                    let name = milestone
                        .get::<Option<String>>("locid")?
                        .map_or(AssetId::default(), |locid| {
                            AssetId::from_key(&format!("awards:{locid}_name"))
                        });
                    effects.borrow_mut().push(AwardEffect::Earned {
                        definition,
                        index,
                        name,
                    });
                } else {
                    complete = false;
                }
                if !whole_group && index < group.raw_len() {
                    complete = false;
                    break;
                }
            }
            if complete && recently_earned {
                if let Some(after) = lua
                    .globals()
                    .get::<Option<Function>>("postawardgroupcomplete")?
                {
                    after.call::<()>(())?;
                }
            }
            Ok(())
        })?,
    )?;
    globals.set("awards_update_bar", scope.create_function(move |_, (_, index, current, target, previous): (Value, usize, f64, f64, Option<f64>)| {
        let previous = previous.unwrap_or(0.0);
        let span = target - previous;
        let progress = if span > 0.0 { ((current - previous) / span).clamp(0.0, 1.0) as f32 } else { 0.0 };
        if progress.is_finite() { effects.borrow_mut().push(AwardEffect::Progress { index, value: progress }); }
        Ok(())
    })?)?;
    globals.set(
        "awards_set_received_graphics",
        scope.create_function(move |_, (_, index, _): (Value, usize, Option<bool>)| {
            effects.borrow_mut().push(AwardEffect::Received(index));
            Ok(())
        })?,
    )?;
    globals.set(
        "awards_set_received",
        globals.get::<Function>("awards_set_received_graphics")?,
    )?;
    Ok(())
}

fn award_animal_rows(
    lua: &Lua,
    facts: &AwardFacts<'_, '_, '_>,
    matches: impl Fn(
        &openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition,
    ) -> bool,
) -> mlua::Result<Table> {
    let rows = lua.create_table()?;
    let Some(definitions) = facts.entities.definitions.get(&facts.entities.assets) else {
        return Ok(rows);
    };
    for (entity, definition, species, _) in
        facts.entities.actors.iter_many(facts.entities()).flatten()
    {
        if species.is_some()
            && definition
                .and_then(|definition| definitions.find_object(definition.0))
                .is_some_and(&matches)
        {
            let row = lua.create_table()?;
            row.set("value", entity.to_bits() as i64)?;
            rows.push(row)?;
        }
    }
    Ok(rows)
}

fn install_award_world_bindings<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    facts: &'env AwardFacts<'env, '_, '_>,
    effects: &'env RefCell<Vec<AwardEffect>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("BFLOG", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    globals.set("SYSTRACE", 0)?;
    globals.set("SYSERROR", 0)?;
    let game_manager = lua.create_table()?;
    game_manager.set(
        "BFG_GET_ENTITIES_DIRECT",
        scope.create_function(move |lua, (_, name): (Table, String)| {
            let rows = lua.create_table()?;
            let key = AssetId::from_key(&name.to_ascii_lowercase());
            let mut index = 1;
            for (entity, definition, _, _) in
                facts.entities.actors.iter_many(facts.entities()).flatten()
            {
                if !definition.is_some_and(|definition| definition.0 == key) {
                    continue;
                }
                let row = lua.create_table()?;
                row.set("value", entity.to_bits() as i64)?;
                rows.set(index, row)?;
                index += 1;
            }
            Ok(rows)
        })?,
    )?;
    let find_direct = game_manager.get::<Function>("BFG_GET_ENTITIES_DIRECT")?;
    let direct_manager = game_manager.clone();
    globals.set(
        "findTypeDirect",
        scope.create_function(move |_, name: String| {
            find_direct.call::<Table>((direct_manager.clone(), name))
        })?,
    )?;
    let entity_methods = lua.create_table()?;
    entity_methods.set("__index", entity_methods.clone())?;
    let area_methods = lua.create_table()?;
    area_methods.set("__index", area_methods.clone())?;
    area_methods.set(
        "BFG_TA_COUNT_BIOME",
        scope.create_function(move |_, (area, biome): (Table, String)| {
            let bits = area.get::<i64>("entity")? as u64;
            let count = Entity::try_from_bits(bits)
                .and_then(|entity| facts.entities.traversable_areas.get(entity).ok())
                .and_then(|area| area.biome_cells.get(&AssetId::from_key(&biome)));
            Ok(count.copied().unwrap_or_default())
        })?,
    )?;
    area_methods.set(
        "BFG_TA_GET_COUNT",
        lua.create_function(|_, area: Table| area.get::<u32>("cells"))?,
    )?;
    entity_methods.set(
        "BFG_GET_TRAVERSABLE_AREA",
        scope.create_function(move |lua, row: Table| {
            let bits = row.get::<i64>("entity")?;
            let Some(area) = Entity::try_from_bits(bits as u64)
                .and_then(|entity| facts.entities.traversable_areas.get(entity).ok())
                .filter(|area| area.total_cells > 0)
            else {
                return Ok(None);
            };
            let value = lua.create_table()?;
            value.set("entity", bits)?;
            value.set("cells", area.total_cells)?;
            value.set_metatable(Some(area_methods.clone()))?;
            Ok(Some(value))
        })?,
    )?;

    entity_methods.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (row, attribute): (Table, String)| {
            let bits = row.get::<i64>("entity")? as u64;
            let Some(entity) = Entity::try_from_bits(bits) else {
                return Ok(0.0);
            };
            let key = AssetId::from_key(&attribute.to_ascii_lowercase());
            if key == AssetId::from_key("f_bonelevel") {
                if let Ok(progress) = facts.entities.fossil_education.get(entity) {
                    return Ok(progress.bone_level);
                }
            }
            let definition = facts
                .entities
                .actors
                .get(entity)
                .ok()
                .and_then(|(_, definition, _, _)| definition);
            Ok(definition
                .and_then(|definition| facts.authored_float(definition.0, key))
                .unwrap_or_default())
        })?,
    )?;
    globals.set(
        "resolveTable",
        scope.create_function(move |lua, bits: i64| {
            let Some(entity) = Entity::try_from_bits(bits as u64) else {
                return Ok(None);
            };
            if !facts.entities.actors.contains(entity)
                || !facts.entities().any(|member| member == entity)
            {
                return Ok(None);
            }
            let state = lua.create_table()?;
            state.set("entity", bits)?;
            state.set_metatable(Some(entity_methods.clone()))?;
            Ok(Some(state))
        })?,
    )?;
    globals.set(
        "getAnimalsFromBiome",
        scope.create_function(move |lua, name: String| {
            let key = AssetId::from_key(&name);
            award_animal_rows(lua, facts, |object| object.biomes.first() == Some(&key))
        })?,
    )?;
    globals.set(
        "getAnimalsFromLocation",
        scope.create_function(move |lua, (name, anywhere): (String, Option<bool>)| {
            let key = AssetId::from_key("location");
            let pattern = if anywhere.unwrap_or(false) {
                name
            } else {
                format!("_{name}")
            };
            award_animal_rows(lua, facts, |object| {
                object
                    .authored_runtime
                    .string_behavior_properties
                    .iter()
                    .any(|(name, value)| *name == key && value.contains(&pattern))
            })
        })?,
    )?;
    globals.set(
        "getUniqueSpecies",
        scope.create_function(move |lua, rows: Table| {
            let result = lua.create_table()?;
            let mut unique = BTreeSet::new();
            let definitions = facts.entities.definitions.get(&facts.entities.assets);
            let species = facts
                .entities
                .species
                .as_deref()
                .zip(facts.entities.species_assets.as_deref())
                .and_then(|(index, assets)| index.get(assets));
            for row in rows.sequence_values::<Table>() {
                let row = row?;
                let Some(entity) = Entity::try_from_bits(row.get::<i64>("value")? as u64) else {
                    continue;
                };
                if let Ok((_, definition, Some(animal), _)) = facts.entities.actors.get(entity) {
                    let name = definition
                        .and_then(|definition| definitions?.find_object(definition.0))
                        .and_then(|object| {
                            object
                                .authored_runtime
                                .string_behavior_properties
                                .iter()
                                .find_map(|(key, value)| {
                                    (*key == AssetId::from_key("s_species"))
                                        .then_some(value.as_str())
                                })
                        })
                        .or_else(|| {
                            species?
                                .find(animal.species)
                                .map(|species| species.authored_species_name.as_str())
                        });
                    if let Some(name) = name.filter(|name| unique.insert(*name)) {
                        result.push(name)?;
                    }
                }
            }
            Ok(result)
        })?,
    )?;
    globals.set(
        "isFreeform",
        scope.create_function(move |_, ()| Ok(facts.mode == WorldSessionMode::Freeform))?,
    )?;
    globals.set(
        "getNumGuests",
        scope.create_function(move |_, period: Option<i64>| {
            Ok(if period == Some(-1) {
                facts.total_visitors
            } else {
                i64::from(facts.guests)
            })
        })?,
    )?;
    globals.set(
        "howMuchCash",
        scope.create_function(move |_, ()| Ok(facts.cash_dollars))?,
    )?;
    globals.set(
        "getCurrentMonth",
        scope.create_function(move |_, ()| Ok(facts.month))?,
    )?;
    globals.set(
        "howManyAnimalsInZoo",
        scope.create_function(move |_, ()| Ok(facts.animals))?,
    )?;
    globals.set(
        "getAnimalsReleased",
        scope.create_function(move |_, ()| Ok(facts.released))?,
    )?;
    globals.set(
        "getDonations",
        scope.create_function(move |_, name: String| {
            let category = if name.eq_ignore_ascii_case("Show") {
                AssetId::from_key("show-donation")
            } else if name.eq_ignore_ascii_case("Education") {
                AssetId::from_key("donate_education")
            } else {
                AssetId::from_key(&format!("Donate_{name}"))
            };
            Ok(facts
                .entities
                .donations
                .get(facts.root)
                .ok()
                .and_then(|summary| summary.total(Some(category)))
                .map_or(0.0, |total| total.amount.dollars()))
        })?,
    )?;
    globals.set(
        "getDonationsAllAnimals",
        scope.create_function(move |_, ()| {
            let Some((species, assets)) = facts
                .entities
                .species
                .as_deref()
                .zip(facts.entities.species_assets.as_deref())
            else {
                return Ok(0.0);
            };
            let (Some(species), Ok(summary)) = (
                species.get(assets),
                facts.entities.donations.get(facts.root),
            ) else {
                return Ok(0.0);
            };
            let categories: BTreeSet<_> = species
                .species()
                .flat_map(|record| {
                    std::iter::once(record.authored_species_name.as_str()).chain(
                        species.variants(record.id).filter_map(move |variant| {
                            species
                                .find_variant_binding(variant.id)?
                                .authored_species_name_override
                                .as_deref()
                        }),
                    )
                })
                .map(|name| AssetId::from_key(&format!("Donate_{name}")))
                .collect();
            Ok(categories
                .into_iter()
                .filter_map(|category| summary.total(Some(category)))
                .map(|total| f64::from(total.amount.dollars()))
                .sum::<f64>())
        })?,
    )?;
    globals.set(
        "countType",
        scope.create_function(move |_, name: String| Ok(facts.count_type(&name)))?,
    )?;
    globals.set(
        "totalIncomeOfStation",
        scope.create_function(move |_, name: String| Ok(facts.station_totals(&name).0))?,
    )?;
    globals.set(
        "totalUsersOfStation",
        scope.create_function(move |_, name: String| Ok(facts.station_totals(&name).1))?,
    )?;
    globals.set(
        "maxStationTypeStarRating",
        scope.create_function(move |_, name: String| Ok(facts.station_totals(&name).2))?,
    )?;
    globals.set(
        "hasShowOfStarRatingX",
        scope.create_function(move |_, stars: f32| {
            Ok(facts
                .entities
                .show_ratings
                .iter_many(facts.members.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .any(|rating| rating.0.is_some_and(|rating| rating >= stars)))
        })?,
    )?;
    globals.set(
        "getLocID",
        scope.create_function(move |lua, key: String| {
            lua.create_string(
                facts
                    .localization
                    .and_then(|locale| locale.find_plain_localized_text(AssetId::from_key(&key)))
                    .unwrap_or(&key),
            )
        })?,
    )?;
    for (kind, getter, setter) in [
        (false, "getglobalvar", "setglobalvar"),
        (true, "getuservar", "setuservar"),
    ] {
        globals.set(
            getter,
            scope.create_function(move |_, key: String| {
                let key = AssetId::from_key(&key);
                for effect in effects.borrow().iter().rev() {
                    match effect {
                        AwardEffect::WorldVariable(id, value) if !kind && *id == key => {
                            return Ok(value.clone());
                        }
                        AwardEffect::UserVariable(id, value) if kind && *id == key => {
                            return Ok(value.clone());
                        }
                        _ => {}
                    }
                }
                Ok(if kind {
                    facts
                        .profile
                        .and_then(|profile| profile.get(key))
                        .map(str::to_owned)
                } else {
                    facts.globals.0.get(&key).cloned()
                })
            })?,
        )?;
        globals.set(
            setter,
            scope.create_function(move |_, (key, value): (String, Option<String>)| {
                let key = AssetId::from_key(&key);
                effects.borrow_mut().push(if kind {
                    AwardEffect::UserVariable(key, value)
                } else {
                    AwardEffect::WorldVariable(key, value)
                });
                Ok(())
            })?,
        )?;
    }
    globals.set(
        "incrementAwardPoints",
        scope.create_function(move |_, amount: i32| {
            effects.borrow_mut().push(AwardEffect::AwardPoints(amount));
            Ok(())
        })?,
    )?;
    globals.set(
        "saveOptions",
        scope.create_function(move |_, ()| {
            effects.borrow_mut().push(AwardEffect::SaveOptions);
            Ok(())
        })?,
    )?;
    globals.set(
        "showawardpanel",
        scope.create_function(move |_, _: MultiValue| {
            effects.borrow_mut().push(AwardEffect::ShowPanel);
            Ok(())
        })?,
    )?;
    globals.set(
        "showpersistentawardpanel",
        globals.get::<Function>("showawardpanel")?,
    )?;
    globals.set(
        "genericokpaneltext",
        scope.create_function(move |_, (_, text): (Value, String)| {
            effects.borrow_mut().push(AwardEffect::Notify(text));
            Ok(())
        })?,
    )?;
    globals.set(
        "placeCratedObjectAtGate",
        scope.create_function(move |_, name: String| {
            effects
                .borrow_mut()
                .push(AwardEffect::Crate(AssetId::from_key(
                    &name.to_ascii_lowercase(),
                )));
            Ok(())
        })?,
    )?;
    globals.set(
        "giveCash",
        scope.create_function(move |_, dollars: f64| {
            effects.borrow_mut().push(AwardEffect::Cash(dollars));
            Ok(())
        })?,
    )?;
    globals.set(
        "showchallengewin",
        scope.create_function(move |_, values: MultiValue| {
            let arguments = values
                .into_iter()
                .filter_map(|value| value.as_string().map(|value| value.to_string_lossy()))
                .collect();
            effects
                .borrow_mut()
                .push(AwardEffect::ChallengeWon(arguments));
            Ok(())
        })?,
    )?;
    let status = lua.create_table()?;
    status.set(
        "BFG_GET_ATTR_FLOAT",
        scope.create_function(move |_, (_, argument): (Table, Table)| {
            let definition = argument.get::<String>("key")?;
            let attribute = argument.get::<String>("val")?;
            Ok(facts
                .authored_float(
                    AssetId::from_key(&definition),
                    AssetId::from_key(&attribute.to_ascii_lowercase()),
                )
                .unwrap_or_default())
        })?,
    )?;
    status.set(
        "ZT_GET_ENTITY_TYPE_INDEX",
        scope.create_function(move |_, (_, key): (Table, String)| Ok(facts.count_type(&key)))?,
    )?;
    status.set(
        "ZT_GET_ALL_ANIMALS_RELEASED",
        scope.create_function(move |lua, _: Table| {
            let result = lua.create_table()?;
            if let Ok(history) = facts.entities.history.get(facts.root) {
                for (index, name) in history.released_species.values().enumerate() {
                    let row = lua.create_table()?;
                    row.set("value", name.as_str())?;
                    result.set(index + 1, row)?;
                }
            }
            Ok(result)
        })?,
    )?;
    for (name, getter) in [
        (
            "ZT_GET_NUM_DINOS_TRANQD",
            (|history: &AwardDomainHistory| history.dinosaur_tranquilizations)
                as fn(&AwardDomainHistory) -> u32,
        ),
        ("ZT_NUM_TRAINING_PERFECTS", |history| {
            history.perfect_training_sessions
        }),
        ("ZT_NUM_SUPER_CLONES", |history| history.super_clones),
        ("ZT_GET_NUM_EXTINCT_SPECIES_RELEASED", |history| {
            u32::try_from(history.extinct_released_species.len()).unwrap_or(u32::MAX)
        }),
        ("ZT_GET_NUM_DISEASES_CURED", |history| {
            history.cured_diseases
        }),
    ] {
        status.set(
            name,
            scope.create_function(move |_, _: Table| {
                Ok(facts.entities.history.get(facts.root).map_or(0, getter))
            })?,
        )?;
    }
    let fossil_manager = lua.create_table()?;
    fossil_manager.set(
        "ZT_GET_NUM_PUZZLES",
        scope.create_function(move |_, (_, level): (Table, u8)| {
            Ok(facts.fossils_by_level(level, false))
        })?,
    )?;
    fossil_manager.set(
        "ZT_GET_NUM_PUZZLES_READY",
        scope.create_function(move |_, (_, level): (Table, u8)| {
            Ok(facts.fossils_by_level(level, true))
        })?,
    )?;
    fossil_manager.set(
        "ZT_GET_NUM_PUZZLES_BUILT",
        scope.create_function(move |_, _: Table| {
            Ok(facts
                .entities
                .assembled_fossils
                .iter_many(facts.entities())
                .flatten()
                .count())
        })?,
    )?;
    status.set(
        "ZT_GET_IS_CAMPAIGN",
        scope.create_function(move |_, _: Table| Ok(facts.mode == WorldSessionMode::Campaign))?,
    )?;
    status.set(
        "ZT_PROCESS_FAME_UPDATE",
        scope.create_function(move |_, _: MultiValue| {
            effects.borrow_mut().push(AwardEffect::RefreshFame);
            Ok(())
        })?,
    )?;
    status.set(
        "ZT_ADD_ADDITIONAL_AMBIENT",
        scope.create_function(move |_, (_, name): (Table, String)| {
            effects
                .borrow_mut()
                .push(AwardEffect::Ambient(AssetId::from_key(
                    &name.to_ascii_lowercase(),
                )));
            Ok(())
        })?,
    )?;
    status.set(
        "ZT_REPLACE_ZOO_WALLS",
        scope.create_function(move |_, (_, name): (Table, String)| {
            effects
                .borrow_mut()
                .push(AwardEffect::ReplaceWalls(AssetId::from_key(&name)));
            Ok(())
        })?,
    )?;
    globals.set(
        "queryObject",
        scope.create_function(move |_, name: String| {
            Ok(match name.as_str() {
                "ZTStatus" | "ZTWorldMgr" | "ZTDiseaseMgr" | "ZTAIAmbientsMgr" => {
                    Some(status.clone())
                }
                "ZTPuzzleMgr" => Some(fossil_manager.clone()),
                "BFGManager" => Some(game_manager.clone()),
                _ => None,
            })
        })?,
    )?;
    Ok(())
}
