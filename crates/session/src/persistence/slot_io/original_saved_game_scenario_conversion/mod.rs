use std::{collections::BTreeSet, io};

use openzt2_game_assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};
use openzt2_game_data::{
    world_scenario::{ScenarioDefinitionRecord, ScenarioScriptBinding, ScenarioScriptPhase},
    AssetId,
};

use crate::scenario::scenario_lua_snapshot_types::{
    saved_scenario_action_key, ScenarioLuaSavedState, ScenarioLuaSavedValue,
    SAVED_SCENARIO_ACTION_ARGUMENTS, SCENARIO_GLOBAL_VARIABLES,
};

pub(super) fn read_original_saved_scenario_lua_state(
    manager: &OrderedSourceDocumentNode,
    definition: &ScenarioDefinitionRecord,
    bindings: &[ScenarioScriptBinding],
) -> io::Result<ScenarioLuaSavedState> {
    if manager.name != "ZTScenarioMgr" {
        return Err(invalid("expected ZTScenarioMgr"));
    }
    let globals = unique_child(manager, "globalvars")?
        .element_children()
        .map(|entry| {
            if entry.name != "var" {
                return Err(invalid("invalid scenario global variable record"));
            }
            Ok((
                ScenarioLuaSavedValue::Bytes(required(entry, "key")?.as_bytes().to_vec()),
                ScenarioLuaSavedValue::Bytes(required(entry, "value")?.as_bytes().to_vec()),
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut arguments = Vec::new();
    let mut modules = Vec::new();
    let mut seen_modules = BTreeSet::new();
    let mut seen_actions = BTreeSet::new();
    let mut restore_action = |action: &OrderedSourceDocumentNode, objective, phase| {
        if action.name != "BFScenarioScriptAction" {
            return Err(invalid("saved scenario contains an unsupported action"));
        }
        let script = required(action, "script")?;
        let entry = required(action, "entry")?;
        let script_key = AssetPath::new(script).key();
        let mut matching = bindings.iter().filter(|binding| {
            binding.scenario == definition.id
                && binding.objective == objective
                && binding.phase == phase
                && binding.entry == entry
                && AssetPath::new(&binding.script).key() == script_key
        });
        let binding = matching
            .next()
            .filter(|_| matching.next().is_none())
            .ok_or_else(|| {
                invalid(format!(
                    "saved scenario action {script}:{entry} has no unique authored binding"
                ))
            })?;
        let key = saved_scenario_action_key(binding);
        if !seen_actions.insert(key) {
            return Err(invalid(format!(
                "saved scenario repeats action {script}:{entry}"
            )));
        }
        arguments.push((
            ScenarioLuaSavedValue::Bytes(key.0.to_vec()),
            read_script_argument(action)?,
        ));
        if seen_modules.insert(script_key) {
            modules.push(binding.script.clone());
        }
        Ok(())
    };
    for (id, node) in saved_scenario_nodes(manager, definition)? {
        for (name, phase) in [
            ("evaluate", ScenarioScriptPhase::Evaluate),
            ("success", ScenarioScriptPhase::Success),
            ("failure", ScenarioScriptPhase::Failure),
        ] {
            for container in node.element_children().filter(|child| child.name == name) {
                for action in container.element_children() {
                    restore_action(action, id, phase)?;
                }
            }
        }
    }
    for container in manager
        .element_children()
        .filter(|node| node.name == "alwaysrunaction")
    {
        for action in container.element_children() {
            restore_action(action, definition.id, ScenarioScriptPhase::AlwaysRun)?;
        }
    }
    Ok(ScenarioLuaSavedState {
        modules,
        globals: vec![
            (
                SAVED_SCENARIO_ACTION_ARGUMENTS.as_bytes().to_vec(),
                ScenarioLuaSavedValue::Table(arguments),
            ),
            (
                SCENARIO_GLOBAL_VARIABLES.as_bytes().to_vec(),
                ScenarioLuaSavedValue::Table(globals),
            ),
        ],
    })
}

pub(super) fn saved_scenario_nodes<'a>(
    manager: &'a OrderedSourceDocumentNode,
    definition: &ScenarioDefinitionRecord,
) -> io::Result<Vec<(AssetId, &'a OrderedSourceDocumentNode)>> {
    fn visit<'a>(
        node: &'a OrderedSourceDocumentNode,
        id: AssetId,
        definition: &ScenarioDefinitionRecord,
        output: &mut Vec<(AssetId, &'a OrderedSourceDocumentNode)>,
    ) -> io::Result<()> {
        if let Some(group) = definition
            .objective_groups
            .iter()
            .find(|group| group.id == id)
        {
            if !matches!(node.name.as_str(), "ZTScenarioMgr" | "BFScenarioGroup") {
                return Err(invalid("saved scenario group has the wrong record type"));
            }
            let children = unique_child(node, "children")?;
            let mut saved = children.element_children();
            for member in &group.members {
                let child = saved
                    .next()
                    .ok_or_else(|| invalid("saved scenario group omits a child"))?;
                visit(child, *member, definition, output)?;
            }
            if saved.next().is_some() {
                return Err(invalid("saved scenario group has additional children"));
            }
        } else {
            let Some(rule) = definition.objectives.iter().find(|rule| rule.id == id) else {
                return Err(invalid("saved scenario child has no authored record"));
            };
            if node.name != "BFScenarioRule"
                || rule.name != Some(AssetId::from_key(required(node, "name")?))
            {
                return Err(invalid(
                    "saved scenario rule disagrees with its authored record",
                ));
            }
        }
        output.push((id, node));
        Ok(())
    }

    let mut output = Vec::new();
    if let Some(root) = definition.objective_groups.last() {
        visit(manager, root.id, definition, &mut output)?;
    } else {
        let mut seen = BTreeSet::new();
        for rule in unique_child(manager, "children")?.element_children() {
            let name = AssetId::from_key(required(rule, "name")?);
            let mut authored = definition
                .objectives
                .iter()
                .filter(|rule| rule.name == Some(name));
            let id = authored
                .next()
                .filter(|_| authored.next().is_none())
                .ok_or_else(|| invalid("saved scenario rule has no unique authored record"))?
                .id;
            if !seen.insert(id) {
                return Err(invalid("saved scenario repeats an authored rule"));
            }
            visit(rule, id, definition, &mut output)?;
        }
    }
    if output.len() != definition.objectives.len() + definition.objective_groups.len() {
        return Err(invalid("saved scenario omits authored nodes"));
    }
    Ok(output)
}

pub(super) fn read_saved_scenario_ui(
    manager: &OrderedSourceDocumentNode,
) -> io::Result<crate::scenario::scenario_lua_ui::ScenarioUiSavedState> {
    let mut saved = crate::scenario::scenario_lua_ui::ScenarioUiSavedState::default();
    if let Some(modifications) = manager
        .element_children()
        .find(|child| child.name == "uimods")
    {
        for modification in modifications.element_children() {
            let name = required(modification, "name")?.to_ascii_lowercase();
            let state = unique_child(unique_child(modification, "new")?, "UIState")?;
            if let Some(active) = state.attribute("active") {
                let enabled = match active {
                    "normal" | "true" => true,
                    "disabled" | "false" => false,
                    value => {
                        return Err(invalid(format!(
                            "invalid saved UI interaction state {value}"
                        )))
                    }
                };
                saved.enabled.insert(name.clone(), enabled);
            }
            if let Some(visible) = state.attribute("visible") {
                let shown = match visible {
                    "normal" | "true" => true,
                    "hidden" | "false" => false,
                    value => return Err(invalid(format!("invalid saved UI visibility {value}"))),
                };
                saved.shown.insert(name, shown);
            }
        }
    }
    saved.continuation = unique_child(manager, "globalvars")?
        .element_children()
        .find(|entry| entry.attribute("key") == Some("nextScenario"))
        .and_then(|entry| entry.attribute("value"))
        .filter(|value| !value.is_empty())
        .map(AssetId::from_key);
    Ok(saved)
}

fn read_script_argument(action: &OrderedSourceDocumentNode) -> io::Result<ScenarioLuaSavedValue> {
    read_script_table(unique_child(action, "script")?, 0)
}

fn read_script_table(
    table: &OrderedSourceDocumentNode,
    depth: usize,
) -> io::Result<ScenarioLuaSavedValue> {
    if depth > 32 {
        return Err(invalid("saved Lua table is nested too deeply"));
    }
    let mut values = Vec::new();
    let mut names = BTreeSet::new();
    let mut indexes = BTreeSet::new();
    for field in table.element_children() {
        let name = required(field, "name")?;
        if name.starts_with('_') || field.name == "nil" {
            continue;
        }
        let key = if name.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            let digits = name
                .as_bytes()
                .iter()
                .take_while(|value| value.is_ascii_digit())
                .count();
            let index = name[..digits]
                .parse::<i32>()
                .map_err(|_| invalid(format!("invalid saved Lua index {name}")))?;
            if !indexes.insert(index) {
                return Err(invalid(format!("saved script repeats index {index}")));
            }
            ScenarioLuaSavedValue::Integer(i64::from(index))
        } else {
            if !names.insert(name) {
                return Err(invalid(format!("saved script repeats field {name}")));
            }
            ScenarioLuaSavedValue::Bytes(name.as_bytes().to_vec())
        };
        values.push((key, read_value(field, depth)?));
    }
    Ok(ScenarioLuaSavedValue::Table(values))
}

fn read_value(node: &OrderedSourceDocumentNode, depth: usize) -> io::Result<ScenarioLuaSavedValue> {
    Ok(match node.name.as_str() {
        "boolean" => ScenarioLuaSavedValue::Boolean(match required(node, "value")? {
            "true" => true,
            "false" => false,
            _ => return Err(invalid("saved Lua boolean is invalid")),
        }),
        "number" => {
            let value = required(node, "value")?
                .parse::<f64>()
                .map_err(|_| invalid("saved Lua number is invalid"))?;
            if !value.is_finite() {
                return Err(invalid("saved Lua number is not finite"));
            }
            ScenarioLuaSavedValue::Number(value)
        }
        "string" => ScenarioLuaSavedValue::Bytes(required(node, "value")?.as_bytes().to_vec()),
        "table" => read_script_table(node, depth + 1)?,
        other => {
            return Err(invalid(format!(
                "saved Lua value {other} requires an unsupported reference or value representation"
            )));
        }
    })
}

fn unique_child<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> io::Result<&'a OrderedSourceDocumentNode> {
    let mut children = node.element_children().filter(|child| child.name == name);
    let child = children
        .next()
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))?;
    if children.next().is_some() {
        return Err(invalid(format!("{} repeats {name}", node.name)));
    }
    Ok(child)
}

fn required<'a>(node: &'a OrderedSourceDocumentNode, name: &str) -> io::Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
