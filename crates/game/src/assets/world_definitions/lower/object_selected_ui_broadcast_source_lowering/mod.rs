use super::source_element_tree_search::authored_type_family_components;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectSelectedUiBroadcast, WorldObjectSelectedUiOperation,
};

pub(super) fn lower_object_selected_ui_broadcasts(
    record: &RecordView<'_, '_>,
) -> Vec<WorldObjectSelectedUiBroadcast> {
    let Some(selected) = authored_type_family_components(record, "ZTTriggeredEventsComponent")
        .into_iter()
        .find_map(|component| {
            component
                .element_children()
                .find(|child| child.name.as_str() == "entity_selected")
        })
    else {
        return Vec::new();
    };
    selected
        .element_children()
        .filter(|child| child.name.as_str() == "broadcast")
        .flat_map(|broadcast| broadcast.element_children())
        .filter(|event| event.attribute_named_any(&["msg"]) == Some("UI_CHILD"))
        .filter_map(|event| {
            let target_name = event.attribute_named_any(&["name"])?;
            let child = event
                .element_children()
                .find(|child| child.name.as_str() == "child")?;
            let operation = match child.attribute_named_any(&["msg"])? {
                "UI_SHOW" => WorldObjectSelectedUiOperation::SetVisible(true),
                "UI_HIDE" => WorldObjectSelectedUiOperation::SetVisible(false),
                "UI_ACTIVATE_ON" => WorldObjectSelectedUiOperation::SetActive(true),
                "UI_ACTIVATE_OFF" => WorldObjectSelectedUiOperation::SetActive(false),
                message => {
                    bevy::log::warn!(
                        definition = record.key,
                        message,
                        "unsupported entity-selected UI broadcast"
                    );
                    return None;
                }
            };
            Some(WorldObjectSelectedUiBroadcast {
                target_name: target_name.to_owned(),
                operation,
            })
        })
        .collect()
}
