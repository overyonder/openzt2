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
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    simulation_time::simulation_clock_types::{ZooClock, ZooMonth},
    world_spawn::{
        persistent_id_types::PersistentIdAllocator,
        selected_world_identity::SelectedWorldIdentity,
        world_load_completion_marker::WorldLoadCompleted,
        world_membership_types::{WorldMember, WorldMembers, WorldRoot},
    },
};
use openzt2_zoo_state::{
    economy::{
        account_transaction_types::{Account, TransactionKind, TransactionRequest},
        authored_transaction_amount::authored_currency_to_money,
        monthly_finance_types::{FinanceMetric, FinancePeriod, MonthlyFinanceHistory},
        zoo_cash_types::ZooCash,
    },
    progression::profile_scenario_variables::{
        PersistProfileScenarioVariables, ProfileScenarioVariables,
    },
};

use openzt2_zoo_state::{
    donations::donation_payment_types::DonationCategoryTotals,
    shows::show_schedule_types::ScheduledShowRating,
};

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AwardWorldEntities<'w, 's> {
    donations: Query<'w, 's, &'static DonationCategoryTotals>,
    show_ratings: Query<'w, 's, &'static ScheduledShowRating>,
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

#[derive(Component, Default)]
pub struct AuthoredAwardPoints(pub u32);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredAwardName(pub AssetId);

#[derive(Component, Default)]
pub struct AwardGlobalVariables(pub BTreeMap<AssetId, String>);

/// A challenge or result panel currently owns the award presentation slot.
#[derive(Component)]
pub struct AwardProgramSuspended;

#[derive(Component, Default)]
pub(super) struct AwardProgramCursor(usize);

#[derive(Component, Default)]
pub struct AwardGroupPresentation {
    pub document: String,
    pub milestones: BTreeMap<usize, AwardMilestonePresentation>,
}

#[derive(Default)]
pub struct AwardMilestonePresentation {
    pub progress: f32,
    pub received: bool,
}

#[derive(Message)]
pub enum AwardProgramEffect {
    ShowAwardPanel {
        root: Entity,
    },
    PlaceCratedObject {
        root: Entity,
        definition: AssetId,
    },
    ShowChallengeWon {
        root: Entity,
        arguments: Vec<String>,
    },
    ReplaceZooWalls {
        root: Entity,
        definition: AssetId,
    },
    AddAmbientSound {
        root: Entity,
        cue: AssetId,
    },
}

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
#[relationship(relationship_target = WorldAwardPrograms)]
pub struct AwardProgramOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AwardProgramOf, linked_spawn)]
pub struct WorldAwardPrograms(Vec<Entity>);

#[derive(Component)]
pub(super) struct AwardProgram {
    name: String,
    path: String,
    script: Handle<LuaScriptAsset>,
    lua: Option<Lua>,
    modules: BTreeSet<AssetId>,
    initialized: bool,
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
                        program.initialized = false;
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
                                initialized: false,
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
                        program.initialized = false;
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
                mode: identity.mode,
                cash_dollars: cash.0.0 as f64 / 100.0,
                total_visitors: history
                    .filter(|history| history.has_initialized_month())
                    .map_or(0, |history| {
                        history.finance_metric_value(
                            FinanceMetric::TotalUsers,
                            FinancePeriod::Lifetime,
                        )
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
                    AwardEffect::Document(document) => {
                        if presentation.document != document {
                            presentation.document = document;
                        }
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
                        effects
                            .display_after_fame
                            .write(ShowAwardPanelAfterFame(root));
                    }
                    AwardEffect::ShowPanel => {
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
                    AwardEffect::Ambient(cue) => {
                        effects
                            .display
                            .write(AwardProgramEffect::AddAmbientSound { root, cue });
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
    mode: WorldSessionMode,
    cash_dollars: f64,
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

enum AwardEffect {
    Document(String),
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
            if !self.initialized {
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
            self.initialized = true;
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
                effects.borrow_mut().push(AwardEffect::Document(document));
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

fn install_award_world_bindings<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    facts: &'env AwardFacts<'env, '_, '_>,
    effects: &'env RefCell<Vec<AwardEffect>>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set("BFLOG", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    globals.set("SYSTRACE", 0)?;
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
            let category = if name == "Show" {
                AssetId::from_key("show-donation")
            } else {
                AssetId::from_key(&format!("Donate_{name}"))
            };
            let cents = facts
                .entities
                .donations
                .iter_many(facts.members.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
                .filter_map(|totals| totals.aggregate_for_category(Some(category)))
                .fold(0_i64, |total, item| total.saturating_add(item.amount.0));
            Ok(cents as f64 / 100.0)
        })?,
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
                .push(AwardEffect::Ambient(AssetId::from_key(&name)));
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
            Ok((name == "ZTStatus").then(|| status.clone()))
        })?,
    )?;
    Ok(())
}
