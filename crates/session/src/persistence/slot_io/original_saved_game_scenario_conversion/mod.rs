use std::{collections::BTreeSet, io};

use openzt2_game_assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};
use openzt2_game_data::world_scenario::{
    ScenarioDefinitionRecord, ScenarioScriptBinding, ScenarioScriptPhase,
};

use crate::scenario::scenario_lua_snapshot_types::{ScenarioLuaSavedState, ScenarioLuaSavedValue};

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
    let mut modules = BTreeSet::new();
    let mut objectives = BTreeSet::new();
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
        let mut saved_argument = None;
        for (phase_name, phase) in [
            ("evaluate", ScenarioScriptPhase::Evaluate),
            ("success", ScenarioScriptPhase::Success),
            ("failure", ScenarioScriptPhase::Failure),
        ] {
            for container in rule
                .element_children()
                .filter(|node| node.name == phase_name)
            {
                for action in container.element_children() {
                    if action.name != "BFScenarioScriptAction" {
                        return Err(invalid("saved scenario contains an unsupported action"));
                    }
                    let script = required(action, "script")?;
                    let entry = required(action, "entry")?;
                    let matching = bindings
                        .iter()
                        .filter(|binding| {
                            binding.scenario == definition.id
                                && binding.objective == objective.id
                                && binding.phase == phase
                                && binding.entry == entry
                                && AssetPath::new(&binding.script).key()
                                    == AssetPath::new(script).key()
                        })
                        .count();
                    if matching != 1 {
                        return Err(invalid(format!(
                            "saved scenario action {script}:{entry} has no unique authored binding"
                        )));
                    }
                    let data = read_script_argument(action)?;
                    if saved_argument
                        .as_ref()
                        .is_some_and(|existing| existing != &data)
                    {
                        return Err(invalid(format!(
                            "saved rule {name} has distinct action contexts that cannot share one argument table"
                        )));
                    }
                    saved_argument = Some(data);
                    modules.insert(script.to_owned());
                }
            }
        }
        if let Some(argument) = saved_argument {
            arguments.push((
                ScenarioLuaSavedValue::Bytes(objective.id.0.to_vec()),
                argument,
            ));
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
            if action.name != "BFScenarioScriptAction" {
                return Err(invalid("saved always-run action is unsupported"));
            }
            let script = required(action, "script")?;
            let entry = required(action, "entry")?;
            if bindings
                .iter()
                .filter(|binding| {
                    binding.scenario == definition.id
                        && binding.objective == definition.id
                        && binding.phase == ScenarioScriptPhase::AlwaysRun
                        && binding.entry == entry
                        && AssetPath::new(&binding.script).key() == AssetPath::new(script).key()
                })
                .count()
                != 1
            {
                return Err(invalid(
                    "saved always-run action has no unique authored binding",
                ));
            }
            if arguments
                .iter()
                .any(|(key, _)| *key == ScenarioLuaSavedValue::Bytes(definition.id.0.to_vec()))
            {
                return Err(invalid("saved scenario repeats always-run action context"));
            }
            arguments.push((
                ScenarioLuaSavedValue::Bytes(definition.id.0.to_vec()),
                read_script_argument(action)?,
            ));
            modules.insert(script.to_owned());
        }
    }
    Ok(ScenarioLuaSavedState {
        modules: modules.into_iter().collect(),
        globals: vec![(
            b"__openzt2_rule_arguments".to_vec(),
            ScenarioLuaSavedValue::Table(arguments),
        )],
    })
}

fn read_script_argument(action: &OrderedSourceDocumentNode) -> io::Result<ScenarioLuaSavedValue> {
    let script = unique_child(action, "script")?;
    let mut values = Vec::new();
    let mut names = BTreeSet::new();
    for field in script.element_children() {
        let name = required(field, "name")?;
        if name.starts_with('_') {
            continue;
        }
        if name.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            return Err(invalid(
                "saved numeric Lua keys require indexed-key restoration",
            ));
        }
        if !names.insert(name) {
            return Err(invalid(format!("saved script repeats field {name}")));
        }
        values.push((
            ScenarioLuaSavedValue::Bytes(name.as_bytes().to_vec()),
            read_value(field)?,
        ));
    }
    Ok(ScenarioLuaSavedValue::Table(values))
}

fn read_value(node: &OrderedSourceDocumentNode) -> io::Result<ScenarioLuaSavedValue> {
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
        "table" if node.element_children().next().is_none() => {
            ScenarioLuaSavedValue::Table(Vec::new())
        }
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
