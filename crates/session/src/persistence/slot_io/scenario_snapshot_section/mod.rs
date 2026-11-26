pub(super) mod original_saved_game_conversion;
use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use crate::scenario::{
    challenge_offer_types::*,
    scenario_lua_snapshot_types::{
        saved_scenario_action_key, ScenarioLuaSavedState, ScenarioLuaSavedValue,
        SAVED_SCENARIO_ACTION_ARGUMENTS,
    },
    scenario_lua_types::ScenarioLuaContext,
};
use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::{
    lua_script::{lua_script_asset_loading::LuaScriptAsset, lua_script_sources::LuaScriptSources},
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::scenario::scenario_script_timing_types::ScenarioScriptTiming;
use openzt2_zoo_state::scenario::{
    scenario_objective_types::*,
    scenario_session_types::{ActiveScenarioSession, ScenarioDocument, ScenarioSessionState},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScenarioObjectiveSnapshot {
    id: u64,
    definition: AssetId,
    status: u8,
    progress: [i64; 2],
    deadline: Option<u64>,
    visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScenarioSnapshot {
    #[serde(skip)]
    timing: Option<ScenarioScriptTiming>,
    #[serde(skip)]
    challenge: Option<ChallengeSnapshot>,
    #[serde(skip)]
    groups: ScenarioGroupSnapshots,
    version: u8,
    definition: AssetId,
    started_tick: u64,
    state: u8,
    objectives: Vec<ScenarioObjectiveSnapshot>,
    lua: Option<ScenarioLuaSavedState>,
    ui: crate::scenario::scenario_lua_ui::ScenarioUiSavedState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct ScenarioGroupSnapshots {
    session: BTreeMap<AssetId, u8>,
    offer: BTreeMap<AssetId, u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChallengeSnapshot {
    rng: [u64; 2],
    next_evaluation: Option<u64>,
    instant_decided: bool,
    candidates: BTreeMap<AssetId, ScenarioLuaSavedState>,
    offer: Option<ChallengeOfferSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChallengeOfferSnapshot {
    id: u64,
    definition: AssetId,
    expires_tick: u64,
    state: u8,
    objectives: Vec<ScenarioObjectiveSnapshot>,
    text: Option<AssetId>,
    description: Option<AssetId>,
    timing_seconds: Option<[f32; 2]>,
    surface: Option<ScenarioChallengePanelSurface>,
}

#[derive(SystemParam)]
pub(in crate::persistence) struct ScenarioSnapshotCapture<'w, 's> {
    sessions: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static ActiveScenarioSession>,
            Option<&'static ScenarioDocument>,
            Option<&'static ScenarioScriptTiming>,
            Option<&'static ScenarioChallengeSelectionRandomNumberGenerator>,
            Option<&'static ScenarioChallengeManagerNextEvaluationTick>,
            Has<ScenarioInstantChallengeDecided>,
            Option<&'static openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity>,
        ),
        With<WorldRoot>,
    >,
    members: Query<'w, 's, &'static ScenarioObjectives>,
    group_members: Query<'w, 's, &'static ScenarioObjectiveGroups>,
    groups: Query<'w, 's, &'static ScenarioObjectiveGroup>,
    objectives: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static ScenarioObjective,
            &'static ScenarioObjectiveStatus,
            &'static ScenarioObjectiveProgress,
            &'static ScenarioObjectiveAuthoredVisibility,
            Option<&'static ScenarioObjectiveDeadline>,
        ),
    >,
    candidates: Query<'w, 's, (Entity, &'static WorldMember, &'static ScenarioChallengeLuaCandidate)>,
    offers: Query<'w, 's, (
        Entity,
        &'static WorldMember,
        &'static PersistentId,
        &'static ScenarioChallengeOffer,
        &'static ScenarioDocument,
        Option<&'static ScenarioChallengePanelText>,
        Option<&'static ScenarioChallengePanelDescription>,
        Option<&'static ScenarioChallengePanelSurface>,
        Option<&'static ScenarioScriptTiming>,
    )>,
    assets: Option<Res<'w, Assets<WorldScenarioDocumentAsset>>>,
    runtime: Query<'w, 's, &'static ScenarioLuaContext>,
    ui: crate::scenario::scenario_lua_ui::ScenarioUiSnapshotCapture<'w, 's>,
}

pub(super) fn capture(
    parameters: &ScenarioSnapshotCapture,
) -> Result<Option<ScenarioSnapshot>, Failure> {
    let Some((owner, session, selected, timing, rng, next, instant_decided, selection)) =
        parameters.sessions.iter().next()
    else {
        return Ok(None);
    };
    let is_challenge = selection.is_some_and(|selection| {
        selection.mode == openzt2_world_state::game_session_types::WorldSessionMode::Challenge
    });
    if session.is_none() && !is_challenge {
        return Ok(None);
    }
    let saved_lua = |owner| {
        parameters
            .runtime
            .get(owner)
            .map_or(Ok(None), ScenarioLuaContext::capture_saved_state)
            .map_err(|error| {
                warn!(%error, "cannot persist scenario Lua data");
                Failure::CorruptSnapshotSection
            })
    };
    let assets = parameters
        .assets
        .as_deref()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let mut snapshot = ScenarioSnapshot {
        timing: timing.cloned(),
        challenge: None,
        groups: ScenarioGroupSnapshots::default(),
        version: if session.is_some() { 1 } else { 2 },
        definition: session.map_or(AssetId::default(), |session| session.definition),
        started_tick: session.map_or(0, |session| session.started_tick),
        state: session.map_or(0, |session| match session.state {
            ScenarioSessionState::Running => 0,
            ScenarioSessionState::Won => 1,
            ScenarioSessionState::Lost => 2,
        }),
        objectives: Vec::new(),
        ui: parameters.ui.capture(),
        lua: None,
    };
    if let Some(session) = session {
        let definition = selected
            .and_then(|selected| assets.get(&selected.0))
            .and_then(|asset| asset.document.find_scenario_record(session.definition))
            .ok_or(Failure::UnknownAssetIdentifier)?;
        snapshot.objectives = capture_objectives(parameters, owner, definition)?;
        snapshot.groups.session = capture_groups(parameters, owner, definition)?;
        snapshot.lua = saved_lua(owner)?;
    }
    if is_challenge {
        let mut candidates = BTreeMap::new();
        for (entity, member, candidate) in &parameters.candidates {
            if member.root == owner {
                if let Some(lua) = saved_lua(entity)? {
                    candidates.insert(candidate.0, lua);
                }
            }
        }
        let mut offers = parameters
            .offers
            .iter()
            .filter(|(_, member, ..)| member.root == owner);
        let offer =
            if let Some((entity, _, id, offer, source, text, description, surface, timing)) =
                offers.next()
            {
                if offers.next().is_some() {
                    return Err(Failure::CorruptSnapshotSection);
                }
                let definition = assets
                    .get(&source.0)
                    .and_then(|asset| asset.document.find_scenario_record(offer.definition))
                    .ok_or(Failure::UnknownAssetIdentifier)?;
                snapshot.groups.offer = capture_groups(parameters, entity, definition)?;
                Some(ChallengeOfferSnapshot {
                    id: id.0,
                    definition: offer.definition,
                    expires_tick: offer.expires_tick,
                    state: match offer.state {
                        ScenarioChallengeOfferState::Offered => 0,
                        ScenarioChallengeOfferState::Active => 1,
                        ScenarioChallengeOfferState::Declined => 2,
                    },
                    objectives: capture_objectives(parameters, entity, definition)?,
                    text: text.map(|text| text.0),
                    description: description.map(|description| description.0),
                    surface: surface.copied(),
                    timing_seconds: timing.map(ScenarioScriptTiming::elapsed_seconds),
                })
            } else {
                None
            };
        snapshot.challenge = Some(ChallengeSnapshot {
            rng: rng.ok_or(Failure::CorruptSnapshotSection)?.to_raw(),
            next_evaluation: next.map(|next| next.0),
            instant_decided,
            candidates,
            offer,
        });
    }
    Ok(Some(snapshot))
}

fn capture_objectives(
    parameters: &ScenarioSnapshotCapture,
    owner: Entity,
    scenario: &openzt2_game_data::world_scenario::ScenarioDefinitionRecord,
) -> Result<Vec<ScenarioObjectiveSnapshot>, Failure> {
    let mut objectives = Vec::new();
    let Ok(members) = parameters.members.get(owner) else {
        return Ok(objectives);
    };
    for item in parameters.objectives.iter_many(members.iter()) {
        let (id, objective, status, progress, visibility, deadline) =
            item.map_err(|_| Failure::CorruptSnapshotSection)?;
        let definition = scenario
            .objectives
            .get(objective.record_index as usize)
            .ok_or(Failure::UnknownAssetIdentifier)?
            .id;
        objectives.push(ScenarioObjectiveSnapshot {
            id: id.0,
            definition,
            status: match status {
                ScenarioObjectiveStatus::Inactive => 0,
                ScenarioObjectiveStatus::Active => 1,
                ScenarioObjectiveStatus::Satisfied => 2,
                ScenarioObjectiveStatus::Failed => 3,
            },
            progress: [progress.current, progress.target],
            deadline: deadline.map(|value| value.end_tick),
            visible: visibility.0,
        });
    }
    objectives.sort_by_key(|objective| objective.id);
    Ok(objectives)
}

fn capture_groups(
    parameters: &ScenarioSnapshotCapture,
    owner: Entity,
    scenario: &openzt2_game_data::world_scenario::ScenarioDefinitionRecord,
) -> Result<BTreeMap<AssetId, u8>, Failure> {
    let Ok(members) = parameters.group_members.get(owner) else {
        return Ok(BTreeMap::new());
    };
    parameters
        .groups
        .iter_many(members.iter())
        .map(|group| {
            let group = group.map_err(|_| Failure::CorruptSnapshotSection)?;
            let definition = scenario
                .objective_groups
                .get(group.record_index)
                .ok_or(Failure::UnknownAssetIdentifier)?;
            Ok((definition.id, encode_rule_status(group.status)))
        })
        .collect()
}

fn encode_rule_status(status: ScenarioObjectiveStatus) -> u8 {
    match status {
        ScenarioObjectiveStatus::Inactive => 0,
        ScenarioObjectiveStatus::Active => 1,
        ScenarioObjectiveStatus::Satisfied => 2,
        ScenarioObjectiveStatus::Failed => 3,
    }
}

fn decode_rule_status(status: u8) -> ScenarioObjectiveStatus {
    match status {
        0 => ScenarioObjectiveStatus::Inactive,
        1 => ScenarioObjectiveStatus::Active,
        2 => ScenarioObjectiveStatus::Satisfied,
        _ => ScenarioObjectiveStatus::Failed,
    }
}

pub(super) fn encode(snapshot: Option<&ScenarioSnapshot>) -> Result<Vec<u8>, Failure> {
    let Some(snapshot) = snapshot else {
        return Ok(Vec::new());
    };
    validate(snapshot)?;
    let mut bytes = postcard::to_allocvec(snapshot).map_err(|_| Failure::CorruptSnapshotSection)?;
    if let Some(timing) = &snapshot.timing {
        bytes.extend_from_slice(b"STM1");
        for elapsed in timing.elapsed_seconds() {
            bytes.extend_from_slice(&elapsed.to_le_bytes());
        }
    }
    if let Some(challenge) = &snapshot.challenge {
        bytes.extend_from_slice(b"SCL1");
        bytes
            .extend(postcard::to_allocvec(challenge).map_err(|_| Failure::CorruptSnapshotSection)?);
    }
    if snapshot.groups != ScenarioGroupSnapshots::default() {
        bytes.extend_from_slice(b"SGP1");
        bytes.extend(
            postcard::to_allocvec(&snapshot.groups).map_err(|_| Failure::CorruptSnapshotSection)?,
        );
    }
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(Failure::CapacityExceeded);
    }
    Ok(bytes)
}

pub(super) fn decode(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Option<ScenarioSnapshot>, Failure> {
    if section.payload_byte_count == 0 && section.encoded_record_count == 0 {
        return Ok(None);
    }
    if section.payload_byte_count > MAX_PAYLOAD_BYTES as u64 {
        return Err(Failure::CapacityExceeded);
    }
    let (mut snapshot, mut remaining): (ScenarioSnapshot, _) = postcard::take_from_bytes(
        &bytes[section.payload_byte_range_within_container(bytes.len())?],
    )
    .map_err(|_| Failure::CorruptSnapshotSection)?;
    if remaining.starts_with(b"STM1") {
        if remaining.len() < 12 {
            return Err(Failure::CorruptSnapshotSection);
        }
        snapshot.timing = Some(
            ScenarioScriptTiming::from_elapsed_seconds(
                f32::from_le_bytes(
                    remaining[4..8]
                        .try_into()
                        .map_err(|_| Failure::CorruptSnapshotSection)?,
                ),
                f32::from_le_bytes(
                    remaining[8..12]
                        .try_into()
                        .map_err(|_| Failure::CorruptSnapshotSection)?,
                ),
            )
            .ok_or(Failure::CorruptSnapshotSection)?,
        );
        remaining = &remaining[12..];
    }
    if let Some(payload) = remaining.strip_prefix(b"SCL1") {
        let (challenge, tail): (ChallengeSnapshot, _) =
            postcard::take_from_bytes(payload).map_err(|_| Failure::CorruptSnapshotSection)?;
        snapshot.challenge = Some(challenge);
        remaining = tail;
    }
    if let Some(payload) = remaining.strip_prefix(b"SGP1") {
        let (groups, tail): (ScenarioGroupSnapshots, _) =
            postcard::take_from_bytes(payload).map_err(|_| Failure::CorruptSnapshotSection)?;
        snapshot.groups = groups;
        remaining = tail;
    }
    if !remaining.is_empty() {
        return Err(Failure::CorruptSnapshotSection);
    }
    validate(&snapshot)?;
    if section.encoded_record_count != record_count(Some(&snapshot)) {
        return Err(Failure::CorruptSnapshotSection);
    }
    Ok(Some(snapshot))
}

fn validate(snapshot: &ScenarioSnapshot) -> Result<(), Failure> {
    if !matches!(snapshot.version, 1 | 2)
        || snapshot.state > 2
        || (snapshot.version == 1 && snapshot.definition == AssetId::default())
        || (snapshot.version == 2
            && (snapshot.challenge.is_none()
                || snapshot.definition != AssetId::default()
                || !snapshot.objectives.is_empty()
                || snapshot.lua.is_some()
                || snapshot.timing.is_some()))
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    if (snapshot.version == 2 && !snapshot.groups.session.is_empty())
        || (!snapshot.groups.offer.is_empty()
            && snapshot
                .challenge
                .as_ref()
                .and_then(|c| c.offer.as_ref())
                .is_none())
        || snapshot
            .groups
            .session
            .values()
            .chain(snapshot.groups.offer.values())
            .any(|status| *status > 3)
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    let mut ids = BTreeSet::new();
    validate_objectives(&snapshot.objectives, &mut ids)?;
    if let Some(lua) = &snapshot.lua {
        validate_lua(lua)?;
    }
    if let Some(challenge) = &snapshot.challenge {
        if challenge.rng[1] & 1 == 0 || challenge.candidates.len() > 65_536 {
            return Err(Failure::CorruptSnapshotSection);
        }
        for lua in challenge.candidates.values() {
            validate_lua(lua)?;
        }
        if let Some(offer) = &challenge.offer {
            if matches!(offer.id, 0 | u64::MAX)
                || !ids.insert(offer.id)
                || offer.state > 2
                || offer.definition == AssetId::default()
            {
                return Err(Failure::CorruptSnapshotSection);
            }
            if offer
                .timing_seconds
                .is_some_and(|[evaluation, always_run]| {
                    ScenarioScriptTiming::from_elapsed_seconds(evaluation, always_run).is_none()
                })
            {
                return Err(Failure::CorruptSnapshotSection);
            }
            validate_objectives(&offer.objectives, &mut ids)?;
        }
    }
    Ok(())
}

fn validate_lua(lua: &ScenarioLuaSavedState) -> Result<(), Failure> {
    if lua.modules.len() > 1024 || lua.modules.iter().any(|path| path.len() > 1024) {
        return Err(Failure::CorruptSnapshotSection);
    }
    Ok(())
}

fn validate_objectives(
    objectives: &[ScenarioObjectiveSnapshot],
    ids: &mut BTreeSet<u64>,
) -> Result<(), Failure> {
    if objectives.len() > 65_536 {
        return Err(Failure::CapacityExceeded);
    }
    let mut definitions = BTreeSet::new();
    for objective in objectives {
        if matches!(objective.id, 0 | u64::MAX)
            || objective.status > 3
            || !ids.insert(objective.id)
            || !definitions.insert(objective.definition)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn record_count(snapshot: Option<&ScenarioSnapshot>) -> u32 {
    snapshot.map_or(0, |snapshot| {
        1 + persistent_ids(Some(snapshot)).count() as u32
    })
}

pub(super) fn persistent_ids(
    snapshot: Option<&ScenarioSnapshot>,
) -> impl Iterator<Item = u64> + '_ {
    snapshot.into_iter().flat_map(|snapshot| {
        snapshot
            .objectives
            .iter()
            .map(|objective| objective.id)
            .chain(
                snapshot
                    .challenge
                    .as_ref()
                    .and_then(|challenge| challenge.offer.as_ref())
                    .into_iter()
                    .flat_map(|offer| {
                        std::iter::once(offer.id)
                            .chain(offer.objectives.iter().map(|objective| objective.id))
                    }),
            )
    })
}

#[derive(SystemParam)]
pub(in crate::persistence) struct ScenarioSnapshotApplication<'w, 's> {
    scenarios: Option<Res<'w, WorldScenarios>>,
    assets: Option<Res<'w, Assets<WorldScenarioDocumentAsset>>>,
    scripts: Option<Res<'w, Assets<LuaScriptAsset>>>,
    script_sources: Option<Res<'w, LuaScriptSources>>,
    existing_groups: Query<'w, 's, &'static ScenarioObjectiveGroups>,
    existing: Query<
        'w,
        's,
        (Entity, &'static WorldMember),
        Or<(
            With<ScenarioObjective>,
            With<ScenarioChallengeOffer>,
            With<ScenarioChallengeLuaCandidate>,
        )>,
    >,
    documents: Query<'w, 's, &'static SelectedWorldDocumentHandles>,
}

pub(super) struct PreparedScenarioSnapshot {
    snapshot: ScenarioSnapshot,
    campaign: Option<PreparedScenarioRules>,
    offer: Option<PreparedScenarioRules>,
    candidates: BTreeMap<AssetId, (Handle<WorldScenarioDocumentAsset>, ScenarioLuaContext)>,
}

struct PreparedScenarioRules {
    document: Handle<WorldScenarioDocumentAsset>,
    objectives: Vec<(u32, Vec<AssetId>)>,
    groups: Vec<openzt2_game_data::world_scenario::ScenarioObjectiveGroupRecord>,
    runtime: Option<ScenarioLuaContext>,
}

pub(super) fn prepare(
    snapshot: Option<ScenarioSnapshot>,
    expected: AssetId,
    root: Entity,
    parameters: &ScenarioSnapshotApplication,
) -> Result<Option<PreparedScenarioSnapshot>, Failure> {
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    validate(&snapshot)?;
    let assets = parameters
        .assets
        .as_deref()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let view = parameters
        .scenarios
        .as_deref()
        .and_then(|scenarios| scenarios.get(assets))
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let campaign = if snapshot.version == 1 {
        let view = match parameters.documents.get(root) {
            Ok(documents) => view
                .with_document(&documents.start)
                .ok_or(Failure::UnknownAssetIdentifier)?,
            Err(_) => view,
        };
        if view.scenario(expected).map(|scenario| scenario.id) != Some(snapshot.definition) {
            return Err(Failure::UnknownAssetIdentifier);
        }
        Some(prepare_rules(
            &view,
            snapshot.definition,
            &snapshot.objectives,
            &snapshot.groups.session,
            snapshot.lua.as_ref(),
            parameters,
        )?)
    } else {
        None
    };
    let mut candidates = BTreeMap::new();
    let mut offer = None;
    if let Some(challenge) = &snapshot.challenge {
        for (definition, lua) in &challenge.candidates {
            let document = view
                .handle_for_scenario(*definition)
                .cloned()
                .ok_or(Failure::UnknownAssetIdentifier)?;
            let runtime = prepare_runtime(lua, &document, parameters)?;
            candidates.insert(*definition, (document, runtime));
        }
        if let Some(saved) = &challenge.offer {
            offer = Some(prepare_rules(
                &view,
                saved.definition,
                &saved.objectives,
                &snapshot.groups.offer,
                None,
                parameters,
            )?);
        }
    }
    Ok(Some(PreparedScenarioSnapshot {
        snapshot,
        campaign,
        offer,
        candidates,
    }))
}

fn prepare_rules(
    view: &openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenariosView,
    scenario: AssetId,
    saved: &[ScenarioObjectiveSnapshot],
    groups: &BTreeMap<AssetId, u8>,
    lua: Option<&ScenarioLuaSavedState>,
    parameters: &ScenarioSnapshotApplication,
) -> Result<PreparedScenarioRules, Failure> {
    let definition = view
        .scenario(scenario)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    if definition.objectives.len() != saved.len()
        || (!groups.is_empty()
            && (groups.len() != definition.objective_groups.len()
                || definition
                    .objective_groups
                    .iter()
                    .any(|group| !groups.contains_key(&group.id))))
    {
        return Err(Failure::UnknownAssetIdentifier);
    }
    let document = view
        .handle_for_scenario(scenario)
        .cloned()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let mut objectives = Vec::with_capacity(saved.len());
    for objective in saved {
        let index = definition
            .objectives
            .iter()
            .position(|record| record.id == objective.definition)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let prerequisites = &definition.objectives[index].prerequisite_objectives;
        if prerequisites
            .iter()
            .any(|id| !saved.iter().any(|objective| objective.definition == *id))
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        objectives.push((index as u32, prerequisites.clone()));
    }
    let runtime = lua
        .map(|lua| prepare_runtime(lua, &document, parameters))
        .transpose()?;
    Ok(PreparedScenarioRules {
        document,
        objectives,
        groups: definition.objective_groups.clone(),
        runtime,
    })
}

fn prepare_runtime(
    saved: &ScenarioLuaSavedState,
    document: &Handle<WorldScenarioDocumentAsset>,
    parameters: &ScenarioSnapshotApplication,
) -> Result<ScenarioLuaContext, Failure> {
    let Some(legacy_index) = saved
        .globals
        .iter()
        .position(|(key, _)| key.as_slice() == b"__openzt2_rule_arguments")
    else {
        return prepare_current_runtime(saved, document, parameters);
    };
    let mut migrated = saved.clone();
    let (_, ScenarioLuaSavedValue::Table(legacy)) = migrated.globals.remove(legacy_index) else {
        return Err(Failure::CorruptSnapshotSection);
    };
    let document_asset = parameters
        .assets
        .as_deref()
        .and_then(|assets| assets.get(document))
        .ok_or(Failure::UnknownAssetIdentifier)?;
    if !migrated
        .globals
        .iter()
        .any(|(key, _)| key.as_slice() == SAVED_SCENARIO_ACTION_ARGUMENTS.as_bytes())
    {
        migrated.globals.push((
            SAVED_SCENARIO_ACTION_ARGUMENTS.as_bytes().to_vec(),
            ScenarioLuaSavedValue::Table(Vec::new()),
        ));
    }
    let Some((_, ScenarioLuaSavedValue::Table(arguments))) = migrated
        .globals
        .iter_mut()
        .find(|(name, _)| name.as_slice() == SAVED_SCENARIO_ACTION_ARGUMENTS.as_bytes())
    else {
        return Err(Failure::CorruptSnapshotSection);
    };
    let mut known = arguments
        .iter()
        .filter_map(|(key, _)| {
            let ScenarioLuaSavedValue::Bytes(bytes) = key else {
                return None;
            };
            Some(bytes.clone())
        })
        .collect::<BTreeSet<_>>();
    for binding in &document_asset.document.scenario_script_bindings {
        if let Some((_, value)) = legacy.iter().find(|(key, _)| {
            matches!(key, ScenarioLuaSavedValue::Bytes(id) if id.as_slice() == binding.objective.0.as_slice())
        }) {
            let key = saved_scenario_action_key(binding).0.to_vec();
            if known.insert(key.clone()) {
                arguments.push((ScenarioLuaSavedValue::Bytes(key), value.clone()));
            }
        }
    }
    prepare_current_runtime(&migrated, document, parameters)
}

fn prepare_current_runtime(
    saved: &ScenarioLuaSavedState,
    document: &Handle<WorldScenarioDocumentAsset>,
    parameters: &ScenarioSnapshotApplication,
) -> Result<ScenarioLuaContext, Failure> {
    ScenarioLuaContext::prepare_saved_state(
        saved,
        document.id(),
        parameters
            .scripts
            .as_deref()
            .ok_or(Failure::CorruptSnapshotSection)?,
        parameters
            .script_sources
            .as_deref()
            .ok_or(Failure::CorruptSnapshotSection)?,
    )
    .map_err(|error| {
        warn!(%error, "cannot restore scenario Lua data");
        Failure::CorruptSnapshotSection
    })
}

pub(super) fn apply(
    commands: &mut Commands,
    prepared: Option<PreparedScenarioSnapshot>,
    root: Entity,
    allocator: &mut PersistentIdAllocator,
    parameters: &mut ScenarioSnapshotApplication,
) -> Result<(), Failure> {
    let Some(prepared) = prepared else {
        return Ok(());
    };
    for id in persistent_ids(Some(&prepared.snapshot)) {
        allocator
            .reserve_imported(PersistentId(id))
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
    }
    if let Ok(groups) = parameters.existing_groups.get(root) {
        for group in groups.iter() {
            commands.entity(group).try_despawn();
        }
    }
    for (entity, member) in &parameters.existing {
        if member.root == root {
            commands.entity(entity).try_despawn();
        }
    }
    let snapshot = prepared.snapshot;
    let read_authored = |document: &Handle<WorldScenarioDocumentAsset>| {
        parameters
            .assets
            .as_deref()
            .and_then(|assets| assets.get(document))
            .map(|asset| &asset.authored)
            .ok_or(Failure::UnknownAssetIdentifier)
    };
    if let Some(campaign) = prepared.campaign {
        spawn_restored_objectives(
            commands,
            root,
            root,
            snapshot.definition,
            &snapshot.objectives,
            &campaign.objectives,
            &campaign.groups,
            &snapshot.groups.session,
            read_authored(&campaign.document)?,
        );
        commands.entity(root).insert((
            ScenarioDocument(campaign.document),
            ActiveScenarioSession {
                definition: snapshot.definition,
                started_tick: snapshot.started_tick,
                state: match snapshot.state {
                    0 => ScenarioSessionState::Running,
                    1 => ScenarioSessionState::Won,
                    _ => ScenarioSessionState::Lost,
                },
            },
        ));
        if let Some(timing) = snapshot.timing {
            commands.entity(root).insert(timing);
        } else {
            commands
                .entity(root)
                .insert(ScenarioScriptTiming::default());
        }
        commands
            .entity(root)
            .insert(campaign.runtime.unwrap_or_default());
    }
    if let Some(challenge) = snapshot.challenge {
        let mut world = commands.entity(root);
        world.insert(ScenarioChallengeSelectionRandomNumberGenerator::from_raw(
            challenge.rng,
        ));
        if let Some(tick) = challenge.next_evaluation {
            world.insert(ScenarioChallengeManagerNextEvaluationTick(tick));
        } else {
            world.remove::<ScenarioChallengeManagerNextEvaluationTick>();
        }
        if challenge.instant_decided {
            world.insert(ScenarioInstantChallengeDecided);
        } else {
            world.remove::<ScenarioInstantChallengeDecided>();
        }
        let mut contexts = BTreeMap::new();
        for (id, (document, runtime)) in prepared.candidates {
            let entity = commands
                .spawn((
                    WorldMember { root },
                    ScenarioChallengeLuaCandidate(id),
                    ScenarioDocument(document),
                    runtime,
                ))
                .id();
            contexts.insert(id, entity);
        }
        if let Some((saved, prepared)) = challenge.offer.zip(prepared.offer) {
            let authored = read_authored(&prepared.document)?;
            let context = contexts.get(&saved.definition).copied().unwrap_or_else(|| {
                commands
                    .spawn((
                        WorldMember { root },
                        ScenarioChallengeLuaCandidate(saved.definition),
                        ScenarioDocument(prepared.document.clone()),
                    ))
                    .id()
            });
            let mut offer = commands.spawn((
                WorldMember { root },
                PersistentId(saved.id),
                ScenarioDocument(prepared.document),
                ScenarioChallengeRuleContext(context),
                saved
                    .timing_seconds
                    .and_then(|[evaluation, always_run]| {
                        ScenarioScriptTiming::from_elapsed_seconds(evaluation, always_run)
                    })
                    .unwrap_or_default(),
                ScenarioChallengeOffer {
                    definition: saved.definition,
                    expires_tick: saved.expires_tick,
                    state: match saved.state {
                        0 => ScenarioChallengeOfferState::Offered,
                        1 => ScenarioChallengeOfferState::Active,
                        _ => ScenarioChallengeOfferState::Declined,
                    },
                },
            ));
            if let Some(text) = saved.text {
                offer.insert(ScenarioChallengePanelText(text));
            }
            if let Some(description) = saved.description {
                offer.insert(ScenarioChallengePanelDescription(description));
            }
            if let Some(surface) = saved.surface {
                offer.insert(surface);
                if saved.state == 0 {
                    offer.insert(ReopenRestoredScenarioChallengePanel);
                }
            }
            let owner = offer.id();
            spawn_restored_objectives(
                commands,
                root,
                owner,
                saved.definition,
                &saved.objectives,
                &prepared.objectives,
                &prepared.groups,
                &snapshot.groups.offer,
                authored,
            );
        }
    }
    crate::scenario::scenario_lua_ui::restore_saved_scenario_ui(commands, root, snapshot.ui);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn spawn_restored_objectives(
    commands: &mut Commands,
    root: Entity,
    owner: Entity,
    scenario: AssetId,
    objectives: &[ScenarioObjectiveSnapshot],
    authored: &[(u32, Vec<AssetId>)],
    groups: &[openzt2_game_data::world_scenario::ScenarioObjectiveGroupRecord],
    saved_groups: &BTreeMap<AssetId, u8>,
    policies: &openzt2_game_assets::world_scenario::authored_world_scenario_state::AuthoredWorldScenarioState,
) {
    let mut entities = BTreeMap::new();
    for (saved, (index, _)) in objectives.iter().zip(authored) {
        let status = decode_rule_status(saved.status);
        let mut entity = commands.spawn((
            PersistentId(saved.id),
            WorldMember { root },
            ScenarioObjectiveOf(owner),
            ScenarioObjective {
                scenario,
                record_index: *index,
            },
            status,
            ScenarioObjectiveProgress {
                current: saved.progress[0],
                target: saved.progress[1],
            },
            ScenarioObjectiveAuthoredVisibility(saved.visible),
        ));
        if let Some(end_tick) = saved.deadline {
            entity.insert(ScenarioObjectiveDeadline { end_tick });
        }
        entities.insert(saved.definition, entity.id());
    }
    for (saved, (_, prerequisites)) in objectives.iter().zip(authored) {
        for id in prerequisites {
            commands.spawn((
                ScenarioObjectivePrerequisiteFor(entities[&saved.definition]),
                ScenarioObjectiveRequires(entities[id]),
            ));
        }
    }
    crate::scenario::scenario_group_completion::spawn_scenario_objective_groups(
        commands,
        owner,
        groups,
        entities,
        objectives
            .iter()
            .map(|objective| (objective.definition, decode_rule_status(objective.status)))
            .chain(
                saved_groups
                    .iter()
                    .map(|(&id, &status)| (id, decode_rule_status(status))),
            )
            .collect(),
        policies,
    );
}

#[cfg(test)]
mod tests;

impl ScenarioSnapshotApplication<'_, '_> {
    pub(super) fn replaced_entities(&self, root: Entity) -> Vec<Entity> {
        self.existing
            .iter()
            .filter_map(|(entity, member)| (member.root == root).then_some(entity))
            .collect()
    }
}
