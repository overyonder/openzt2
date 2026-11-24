use std::{collections::BTreeSet, io};

use openzt2_game_assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};
use openzt2_game_data::world_scenario::{
    ScenarioDefinitionRecord, ScenarioScriptBinding, ScenarioScriptPhase,
};

use crate::scenario::scenario_lua_snapshot_types::{
    saved_scenario_action_key, ScenarioLuaSavedState, ScenarioLuaSavedValue,
    SAVED_SCENARIO_ACTION_ARGUMENTS,
};

pub(super) fn read_original_saved_scenario_lua_state(
    manager: &OrderedSourceDocumentNode,
    definition: &ScenarioDefinitionRecord,
    bindings: &[ScenarioScriptBinding],
) -> io::Result<ScenarioLuaSavedState> {
    if manager.name != "ZTScenarioMgr" {
        return Err(invalid("expected ZTScenarioMgr"));
    }
    if unique_child(manager, "globalvars")?
        .element_children()
        .next()
        .is_some()
    {
        return Err(invalid(
            "ZTScenarioMgr globalvars restoration is unsupported",
        ));
    }
    let mut arguments = Vec::new();
    let mut modules = Vec::new();
    let mut seen_modules = BTreeSet::new();
    let mut seen_actions = BTreeSet::new();
    let mut objectives = BTreeSet::new();
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
    for rule in unique_child(manager, "children")?.element_children() {
        if rule.name != "BFScenarioRule" {
            return Err(invalid("saved scenario contains an unsupported rule group"));
        }
        let name = required(rule, "name")?;
        let matches = definition
            .objectives
            .iter()
            .filter(|objective| objective.name == Some(openzt2_game_data::AssetId::from_key(name)))
            .collect::<Vec<_>>();
        let [objective] = matches.as_slice() else {
            return Err(invalid(format!(
                "saved scenario rule {name} has no unique authored objective"
            )));
        };
        if !objectives.insert(objective.id) {
            return Err(invalid(format!("saved scenario repeats rule {name}")));
        }
        for (name, phase) in [
            ("evaluate", ScenarioScriptPhase::Evaluate),
            ("success", ScenarioScriptPhase::Success),
            ("failure", ScenarioScriptPhase::Failure),
        ] {
            for container in rule.element_children().filter(|node| node.name == name) {
                for action in container.element_children() {
                    restore_action(action, objective.id, phase)?;
                }
            }
        }
    }
    if objectives.len() != definition.objectives.len() {
        return Err(invalid("saved scenario omits authored objectives"));
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
        globals: vec![(
            SAVED_SCENARIO_ACTION_ARGUMENTS.as_bytes().to_vec(),
            ScenarioLuaSavedValue::Table(arguments),
        )],
    })
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
