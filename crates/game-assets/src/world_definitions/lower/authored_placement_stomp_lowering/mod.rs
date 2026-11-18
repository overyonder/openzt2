use std::collections::BTreeMap;

use openzt2_game_data::{
    AssetId,
    world_definitions::object_placement::placement_stomp_rules::{
        PlacementStompAction, PlacementStompPolicy, PlacementStompRule, PlacementStompTarget,
        PlacementTargetFacts,
    },
};

use crate::source_document::{
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::canonicalize_source_document_record_key,
};

use super::{
    source_element_tree_search::{
        authored_type_family_component_attribute, authored_type_family_components,
        authored_type_family_elements_named, find_descendant,
    },
    world_definition_source_value_reading_and_conversion::id,
};

pub(super) fn lower_authored_placement_stomp_policy(
    record: &RecordView<'_, '_>,
) -> Result<PlacementStompPolicy, BindError> {
    let grid_snap = authored_type_family_component_attribute(record, "ZTPlacementData", "gridSnap")
        .map(|value| {
            placement_boolean(value).ok_or_else(|| {
                BindError::record(record, format!("invalid gridSnap value {value:?}"))
            })
        })
        .transpose()?
        .unwrap_or_default();
    let families = authored_type_family_components(record, "ZTPlacementData")
        .into_iter()
        .filter_map(|placement| find_descendant(placement, "stompData"))
        .map(|stomp| {
            let mut rules = Vec::new();
            for group in stomp.element_children() {
                let action = match canonicalize_source_document_record_key(&group.name).as_str() {
                    "allow" => PlacementStompAction::Allow,
                    "prevent" => PlacementStompAction::Prevent,
                    "delete" => PlacementStompAction::Delete,
                    _ => continue,
                };
                for target in group.element_children() {
                    let name = canonicalize_source_document_record_key(&target.name);
                    let values = target
                        .element_children()
                        .map(|value| placement_property_value_id(&name, &value.name))
                        .collect::<Vec<_>>();
                    let target = if values.is_empty() && !name.starts_with("b_") {
                        PlacementStompTarget::Type(id(&name))
                    } else {
                        PlacementStompTarget::Property {
                            name: id(&name),
                            values: if values.is_empty() {
                                vec![id("true")]
                            } else {
                                values
                            },
                        }
                    };
                    rules.push(PlacementStompRule { action, target });
                }
            }
            rules
        })
        .collect();
    Ok(PlacementStompPolicy {
        grid_snap,
        families,
    })
}

pub(super) fn lower_authored_placement_target_facts(
    record: &RecordView<'_, '_>,
) -> PlacementTargetFacts {
    let mut properties = BTreeMap::new();
    for group in ["BFAIEntityDataShared", "BFAIEntityDataInstance"] {
        for element in authored_type_family_elements_named(record, group)
            .into_iter()
            .rev()
        {
            properties.extend(element.attributes().map(|(name, value)| {
                let name = canonicalize_source_document_record_key(name);
                (id(&name), placement_property_value_id(&name, value))
            }));
        }
    }
    let mut types = record
        .type_tokens()
        .iter()
        .map(|token| id(token))
        .collect::<Vec<_>>();
    let record_id = id(record.key);
    if !types.contains(&record_id) {
        types.push(record_id);
    }
    PlacementTargetFacts { types, properties }
}

fn placement_property_value_id(name: &str, value: &str) -> AssetId {
    if name.starts_with("b_") {
        if let Some(flag) = placement_boolean(value) {
            return id(if flag { "true" } else { "false" });
        }
    }
    id(value)
}

fn placement_boolean(value: &str) -> Option<bool> {
    match canonicalize_source_document_record_key(value).as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}
