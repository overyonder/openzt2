use std::io;

use bevy::prelude::*;
use openzt2_game_assets::{
    lua_script::{
        lua_script_archive_path_index::LuaScriptArchivePathIndex,
        lua_script_asset_loading::LuaScriptAsset,
    },
    source_document::ordered_source_document_types::OrderedSourceDocumentNode,
    world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};
use openzt2_game_data::world_scenario::{ScenarioDefinitionRecord, ScenarioScriptBinding};

use super::super::original_saved_game_scenario_conversion::{
    read_original_saved_scenario_lua_state, read_saved_scenario_ui, saved_scenario_nodes,
};
use super::{ScenarioGroupSnapshots, ScenarioObjectiveSnapshot, ScenarioSnapshot};

pub(in crate::persistence::slot_io) fn read_original_saved_scenario_snapshot(
    manager: &OrderedSourceDocumentNode,
    definition: &ScenarioDefinitionRecord,
    bindings: &[ScenarioScriptBinding],
    document: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
    scripts: &Assets<LuaScriptAsset>,
    script_index: &LuaScriptArchivePathIndex,
    activation_tick: u64,
    mut allocate_identifier: impl FnMut() -> io::Result<u64>,
) -> io::Result<ScenarioSnapshot> {
    if definition.time_limit_ticks != 0 {
        return Err(invalid(
            "saved timed scenario activation/deadline conversion is unavailable",
        ));
    }
    let state = match manager.attribute("state") {
        Some("0") => 0,
        Some("1") => 1,
        Some("-1") => 2,
        Some(value) => {
            return Err(invalid(format!(
                "saved scenario manager state {value} has no active session representation"
            )))
        }
        None => return Err(invalid("saved scenario manager has no state")),
    };
    let mut objectives = Vec::new();
    let mut groups = ScenarioGroupSnapshots::default();
    for (id, node) in saved_scenario_nodes(manager, definition)? {
        let status = match node.attribute("state") {
            Some("2") => 0,
            Some("0") => 1,
            Some("1") => 2,
            Some("-1") => 3,
            _ => return Err(invalid("saved scenario node has an invalid state")),
        };
        if node.name != "BFScenarioRule" {
            groups.session.insert(id, status);
            continue;
        }
        let visible = match node.attribute("hidden") {
            Some("true" | "1") => false,
            Some("false" | "0") => true,
            _ => return Err(invalid("saved scenario rule has no boolean hidden state")),
        };
        objectives.push(ScenarioObjectiveSnapshot {
            id: allocate_identifier()?,
            definition: id,
            status,
            progress: [0, 0],
            deadline: None,
            visible,
        });
    }
    let overlay = read_original_saved_scenario_lua_state(manager, definition, bindings)?;
    let lua = crate::scenario::scenario_lua_snapshot::normalize_original_saved_scenario_lua_state(
        &overlay,
        document,
        scripts,
        script_index,
    )
    .map_err(|error| {
        invalid(format!(
            "cannot restore saved scenario script data: {error}"
        ))
    })?;
    let snapshot = ScenarioSnapshot {
        timing: Some(Default::default()),
        challenge: None,
        groups,
        version: 1,
        definition: definition.id,
        started_tick: activation_tick,
        state,
        objectives,
        lua: Some(lua),
        ui: read_saved_scenario_ui(manager)?,
    };
    super::validate(&snapshot)
        .map_err(|error| invalid(format!("invalid converted scenario: {error:?}")))?;
    Ok(snapshot)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub(in crate::persistence::slot_io) fn assign_original_scenario_objective_identifiers(
    snapshot: &mut Option<ScenarioSnapshot>,
    _root: Entity,
    allocator: &openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator,
) -> Result<u64, super::Failure> {
    use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;

    let Some(snapshot) = snapshot else {
        return Ok(allocator.next());
    };
    super::validate(snapshot)?;
    let mut planned = PersistentIdAllocator::from_snapshot(allocator.next())
        .map_err(|_| super::Failure::BrokenPersistentEntityReference)?;
    let identifiers = snapshot
        .objectives
        .iter()
        .map(|_| {
            planned
                .allocate()
                .map_err(|_| super::Failure::CapacityExceeded)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (objective, identifier) in snapshot.objectives.iter_mut().zip(identifiers) {
        objective.id = identifier.0;
    }
    snapshot
        .objectives
        .sort_unstable_by_key(|objective| objective.id);
    Ok(planned.next())
}
