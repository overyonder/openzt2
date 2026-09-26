use super::source_element_tree_search::authored_type_family_components;
use super::world_definition_source_value_reading_and_conversion::id;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectDetachActionDefinition, WorldObjectDetachDestination,
};

#[cfg(test)]
mod tests;

pub(super) fn lower_object_detach_actions(
    record: &RecordView<'_, '_>,
) -> Vec<WorldObjectDetachActionDefinition> {
    let mut actions: Vec<WorldObjectDetachActionDefinition> = Vec::new();
    for component in authored_type_family_components(record, "BFGDetachInfo") {
        for table in component.element_children().filter(|child| {
            canonicalize_source_document_record_key(child.name.as_str()) == "detachactiontable"
        }) {
            for action in table.element_children() {
                let name = id(action.name.as_str());
                if actions.iter().any(|existing| existing.name == name) {
                    continue;
                }
                let created_objects = action
                    .element_children()
                    .filter(|child| {
                        canonicalize_source_document_record_key(child.name.as_str()) == "createlist"
                    })
                    .flat_map(|list| list.element_children())
                    .map(|created| {
                        (
                            id(created.name.as_str()),
                            detach_destination(created.attribute_named_any(&["destination"])),
                        )
                    })
                    .collect();
                actions.push(WorldObjectDetachActionDefinition {
                    name,
                    destination: detach_destination(action.attribute_named_any(&["destination"])),
                    created_objects,
                });
            }
        }
    }
    actions
}

fn detach_destination(value: Option<&str>) -> WorldObjectDetachDestination {
    match value
        .map(canonicalize_source_document_record_key)
        .as_deref()
    {
        Some("kill") => WorldObjectDetachDestination::Kill,
        Some("drop") => WorldObjectDetachDestination::Drop,
        Some("fall") => WorldObjectDetachDestination::Fall,
        Some(container) => WorldObjectDetachDestination::Container(id(container)),
        None => WorldObjectDetachDestination::Container(Default::default()),
    }
}
