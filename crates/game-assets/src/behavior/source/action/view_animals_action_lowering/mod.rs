use std::io;

use openzt2_game_data::{
    behavior::{
        action::view_animals::{BehaviorViewAnimalsAction, ViewAnimalsEmoteSet},
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_optional_float_attribute, read_optional_trimmed_string_attribute},
};

pub(super) fn lower_view_animals_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "ZTBehViewAnimals" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "viewMeterStart" | "viewMeterStartMin" | "viewMeterStartMax" | "dockVPBeh" | "retarget"
        )
    }) {
        return Err(invalid_behavior_source_data("unmapped view animals policy"));
    }
    let finite = |name: &str| -> io::Result<Option<f32>> {
        Ok(read_optional_float_attribute(node, name)?.filter(|value| value.is_finite()))
    };
    let view_meter_start = finite("viewMeterStart")?
        .filter(|value| *value > 0.0)
        .unwrap_or(60.0);
    let view_meter_start_range = [
        finite("viewMeterStartMin")?.unwrap_or(view_meter_start),
        finite("viewMeterStartMax")?.unwrap_or(view_meter_start),
    ];
    let retarget = read_optional_trimmed_string_attribute(node, "retarget")
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(invalid_behavior_source_data(
                "view animals has invalid retarget",
            )),
        })
        .transpose()?;
    let mut emote_sets = Vec::new();
    for child in node.element_children() {
        if child.name != "emoteSets" || !child.attributes.is_empty() {
            return Err(invalid_behavior_source_data("unmapped view animals child"));
        }
        for set in child.element_children() {
            let bound = |name: &str| -> io::Result<f32> {
                read_optional_float_attribute(set, name)?
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        invalid_behavior_source_data(format!(
                            "view animals emote set has no {name}"
                        ))
                    })
            };
            emote_sets.push(ViewAnimalsEmoteSet {
                behavior_set: AssetId::from_key(&set.name.to_ascii_lowercase()),
                view_meter_range: [bound("min")?, bound("max")?],
            });
        }
    }
    Ok(Some(BehaviorAction::ViewAnimals(
        BehaviorViewAnimalsAction {
            view_meter_start,
            view_meter_start_range,
            dock_viewpoint_behavior_set: read_optional_trimmed_string_attribute(node, "dockVPBeh")
                .map(|name| AssetId::from_key(&name.to_ascii_lowercase())),
            retarget,
            emote_sets,
        },
    )))
}
