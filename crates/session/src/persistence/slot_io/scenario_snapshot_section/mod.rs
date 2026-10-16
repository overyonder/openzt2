use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use crate::scenario::{
    scenario_lua_snapshot_types::ScenarioLuaSavedState,
    scenario_lua_types::ScenarioLuaVirtualMachineContexts,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::{
    lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
    },
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::scenario::{
    scenario_objective_types::*,
    scenario_session_types::{
        ActiveScenarioSession, ScenarioSessionState, SelectedScenarioDocument,
    },
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
    version: u8,
    definition: AssetId,
    started_tick: u64,
    state: u8,
    objectives: Vec<ScenarioObjectiveSnapshot>,
    lua: Option<ScenarioLuaSavedState>,
    ui: crate::scenario::scenario_lua_ui::ScenarioUiSavedState,
}

#[derive(SystemParam)]
pub(in crate::persistence) struct ScenarioSnapshotCapture<'w, 's> {
    sessions: Query<'w, 's, &'static ActiveScenarioSession, With<WorldRoot>>,
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
    scenarios: Option<Res<'w, WorldScenarios>>,
    assets: Option<Res<'w, Assets<WorldScenarioDocumentAsset>>>,
    runtime: Option<NonSend<'w, ScenarioLuaVirtualMachineContexts>>,
    ui: crate::scenario::scenario_lua_ui::ScenarioUiSnapshotCapture<'w>,
}

pub(super) fn capture(
    parameters: &ScenarioSnapshotCapture,
) -> Result<Option<ScenarioSnapshot>, Failure> {
    let Some(session) = parameters.sessions.iter().next() else {
        return Ok(None);
    };
    let assets = parameters
        .assets
        .as_deref()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let view = parameters
        .scenarios
        .as_deref()
        .and_then(|scenarios| scenarios.get(assets))
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let scenario = view
        .scenario(session.definition)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let mut objectives = Vec::new();
    for (id, objective, status, progress, visibility, deadline) in &parameters.objectives {
        if objective.scenario != session.definition {
            continue;
        }
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
    Ok(Some(ScenarioSnapshot {
        version: 1,
        definition: session.definition,
        started_tick: session.started_tick,
        state: match session.state {
            ScenarioSessionState::Running => 0,
            ScenarioSessionState::Won => 1,
            ScenarioSessionState::Lost => 2,
        },
        objectives,
        ui: parameters.ui.capture(),
        lua: parameters
            .runtime
            .as_deref()
            .ok_or(Failure::CorruptSnapshotSection)?
            .capture_saved_scenario_state(session.definition)
            .map_err(|error| {
                warn!(%error, "cannot persist scenario Lua data");
                Failure::CorruptSnapshotSection
            })?,
    }))
}

pub(super) fn encode(snapshot: Option<&ScenarioSnapshot>) -> Result<Vec<u8>, Failure> {
    let Some(snapshot) = snapshot else {
        return Ok(Vec::new());
    };
    validate(snapshot)?;
    let bytes = postcard::to_allocvec(snapshot).map_err(|_| Failure::CorruptSnapshotSection)?;
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
    let (snapshot, remaining): (ScenarioSnapshot, _) = postcard::take_from_bytes(
        &bytes[section.payload_byte_range_within_container(bytes.len())?],
    )
    .map_err(|_| Failure::CorruptSnapshotSection)?;
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
    if snapshot.version != 1
        || snapshot.state > 2
        || snapshot.definition == AssetId::default()
        || snapshot.objectives.len() > 65_536
    {
        return Err(Failure::CorruptSnapshotSection);
    }
    if snapshot.lua.as_ref().is_some_and(|lua| {
        lua.modules.is_empty()
            || lua.modules.len() > 1024
            || lua.modules.iter().any(|path| path.len() > 1024)
    }) {
        return Err(Failure::CorruptSnapshotSection);
    }
    let mut ids = BTreeSet::new();
    let mut definitions = BTreeSet::new();
    for objective in &snapshot.objectives {
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
    snapshot.map_or(0, |snapshot| 1 + snapshot.objectives.len() as u32)
}

pub(super) fn persistent_ids(
    snapshot: Option<&ScenarioSnapshot>,
) -> impl Iterator<Item = u64> + '_ {
    snapshot
        .into_iter()
        .flat_map(|snapshot| snapshot.objectives.iter().map(|objective| objective.id))
}

#[derive(SystemParam)]
pub(in crate::persistence) struct ScenarioSnapshotApplication<'w, 's> {
    scenarios: Option<Res<'w, WorldScenarios>>,
    assets: Option<Res<'w, Assets<WorldScenarioDocumentAsset>>>,
    scripts: Option<Res<'w, Assets<LuaScriptAsset>>>,
    script_index: Option<Res<'w, LuaScriptArchivePathIndex>>,
    runtime: Option<NonSendMut<'w, ScenarioLuaVirtualMachineContexts>>,
    existing: Query<'w, 's, Entity, With<ScenarioObjective>>,
}

pub(super) struct PreparedScenarioSnapshot {
    snapshot: ScenarioSnapshot,
    document: Handle<WorldScenarioDocumentAsset>,
    indices: Vec<u32>,
    prerequisites: Vec<Vec<AssetId>>,
    runtime: Option<crate::scenario::scenario_lua_snapshot::PreparedScenarioLuaRuntime>,
}

pub(super) fn prepare(
    snapshot: Option<ScenarioSnapshot>,
    expected: AssetId,
    parameters: &ScenarioSnapshotApplication,
) -> Result<Option<PreparedScenarioSnapshot>, Failure> {
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    let assets = parameters
        .assets
        .as_deref()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let view = parameters
        .scenarios
        .as_deref()
        .and_then(|scenarios| scenarios.get(assets))
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let definition = view
        .scenario(snapshot.definition)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    if view.scenario(expected).map(|scenario| scenario.id) != Some(definition.id)
        || definition.objectives.len() != snapshot.objectives.len()
    {
        return Err(Failure::UnknownAssetIdentifier);
    }
    let document = view
        .handle_for_scenario(snapshot.definition)
        .cloned()
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let mut indices = Vec::new();
    let mut prerequisites = Vec::new();
    for saved in &snapshot.objectives {
        let index = definition
            .objectives
            .iter()
            .position(|objective| objective.id == saved.definition)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        indices.push(index as u32);
        prerequisites.push(definition.objectives[index].prerequisite_objectives.clone());
    }
    if prerequisites.iter().flatten().any(|id| {
        !snapshot
            .objectives
            .iter()
            .any(|objective| objective.definition == *id)
    }) {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    let runtime = snapshot
        .lua
        .as_ref()
        .map(|saved| {
            if parameters.runtime.is_none() {
                return Err(mlua::Error::runtime("scenario runtime is unavailable"));
            }
            ScenarioLuaVirtualMachineContexts::prepare_saved_scenario_state(
                saved,
                document.id(),
                parameters
                    .scripts
                    .as_deref()
                    .ok_or_else(|| mlua::Error::runtime("scenario scripts unavailable"))?,
                parameters
                    .script_index
                    .as_deref()
                    .ok_or_else(|| mlua::Error::runtime("scenario script index unavailable"))?,
            )
        })
        .transpose()
        .map_err(|error| {
            warn!(%error, "cannot restore scenario Lua data");
            Failure::CorruptSnapshotSection
        })?;
    Ok(Some(PreparedScenarioSnapshot {
        snapshot,
        document,
        indices,
        prerequisites,
        runtime,
    }))
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
    for objective in &prepared.snapshot.objectives {
        allocator
            .reserve_imported(root, PersistentId(objective.id))
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
    }
    for entity in &parameters.existing {
        commands.entity(entity).despawn();
    }
    let snapshot = prepared.snapshot;
    let mut entities = BTreeMap::new();
    for (saved, index) in snapshot.objectives.iter().zip(prepared.indices) {
        let entity = commands
            .spawn((
                PersistentId(saved.id),
                WorldMember { root },
                ScenarioObjective {
                    scenario: snapshot.definition,
                    record_index: index,
                },
                match saved.status {
                    0 => ScenarioObjectiveStatus::Inactive,
                    1 => ScenarioObjectiveStatus::Active,
                    2 => ScenarioObjectiveStatus::Satisfied,
                    _ => ScenarioObjectiveStatus::Failed,
                },
                ScenarioObjectiveProgress {
                    current: saved.progress[0],
                    target: saved.progress[1],
                },
                ScenarioObjectiveAuthoredVisibility(saved.visible),
            ))
            .id();
        if let Some(end_tick) = saved.deadline {
            commands
                .entity(entity)
                .insert(ScenarioObjectiveDeadline { end_tick });
        }
        entities.insert(saved.definition, entity);
    }
    for (saved, prerequisites) in snapshot.objectives.iter().zip(prepared.prerequisites) {
        let links = prerequisites
            .iter()
            .map(|id| entities[id])
            .collect::<Vec<_>>()
            .into_boxed_slice();
        commands
            .entity(entities[&saved.definition])
            .insert(ScenarioObjectivePrerequisiteEntities(links));
    }
    commands.entity(root).insert(ActiveScenarioSession {
        definition: snapshot.definition,
        started_tick: snapshot.started_tick,
        state: match snapshot.state {
            0 => ScenarioSessionState::Running,
            1 => ScenarioSessionState::Won,
            _ => ScenarioSessionState::Lost,
        },
    });
    commands.insert_resource(SelectedScenarioDocument(prepared.document));
    crate::scenario::scenario_lua_ui::restore_saved_scenario_ui(commands, snapshot.ui);
    if let Some(runtime) = prepared.runtime {
        parameters
            .runtime
            .as_deref_mut()
            .ok_or(Failure::CorruptSnapshotSection)?
            .install_saved_scenario_state(snapshot.definition, runtime);
    }
    Ok(())
}

pub(super) fn scripts_are_pending(
    parameters: &ScenarioSnapshotApplication,
    server: &AssetServer,
) -> bool {
    parameters
        .script_index
        .as_deref()
        .zip(parameters.scripts.as_deref())
        .is_some_and(|(index, scripts)| index.has_pending_script_loads(server, scripts))
}

#[cfg(test)]
mod tests;
