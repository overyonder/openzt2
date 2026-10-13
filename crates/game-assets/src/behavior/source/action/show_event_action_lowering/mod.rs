use std::io;

use openzt2_game_data::{
    behavior::{action::show_event::BehaviorShowEventAction, action_record::BehaviorAction},
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_boolean_attribute_or_false, read_optional_float_attribute},
};

pub(super) fn lower_animal_show_event_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehShowEvent" {
        return Ok(None);
    }
    // Only the task-scoped animal form has an execution owner here.
    if node.attribute("Type") != Some("AnimalEvent")
        || !read_boolean_attribute_or_false(node, "ExpireWithTask")?
        || node.attribute("Interrupt").is_none()
        || node.element_children().next().is_some()
        || node.attributes.iter().any(|attribute| {
            !matches!(
                attribute.name(),
                "Type"
                    | "Name"
                    | "Priority"
                    | "Interrupt"
                    | "ExpireWithTask"
                    | "Timein"
                    | "Timeout"
            )
        })
    {
        return Err(invalid_behavior_source_data(
            "unsupported show event policy",
        ));
    }
    let name = node
        .attribute("Name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid_behavior_source_data("show event requires Name"))?;
    let priority = node
        .attribute("Priority")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| invalid_behavior_source_data("show event requires unsigned Priority"))?;
    let seconds = |name| -> io::Result<Option<f32>> {
        let value = read_optional_float_attribute(node, name)?;
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(invalid_behavior_source_data("invalid show event time"));
        }
        Ok(value)
    };
    Ok(Some(BehaviorAction::AnimalShowEvent(
        BehaviorShowEventAction {
            name: AssetId::from_key(&name.to_ascii_lowercase()),
            priority,
            interrupt: read_boolean_attribute_or_false(node, "Interrupt")?,
            expire_with_task: true,
            time_in_seconds: seconds("Timein")?,
            timeout_seconds: seconds("Timeout")?,
        },
    )))
}
