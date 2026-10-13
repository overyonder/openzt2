//! Lower authored hiding, such as a guest inside a gift shop.

use super::{invalid_behavior_source_data, source_value_reading::read_optional_float_attribute};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use std::{io, time::Duration};

pub(super) fn lower_hide_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehHide" {
        return Ok(None);
    }
    if node
        .attributes
        .iter()
        .any(|attribute| !matches!(attribute.name(), "minTime" | "maxTime"))
        || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehHide has unmapped policy",
        ));
    }
    let duration = |name| -> io::Result<Duration> {
        let seconds = read_optional_float_attribute(node, name)?.unwrap_or(0.0);
        Duration::try_from_secs_f64(f64::from(seconds)).map_err(|_| {
            invalid_behavior_source_data("hide duration must be finite and nonnegative")
        })
    };
    let (minimum, maximum) = (duration("minTime")?, duration("maxTime")?);
    if maximum < minimum {
        return Err(invalid_behavior_source_data(
            "hide maximum is below minimum",
        ));
    }
    Ok(Some(BehaviorAction::Hide { minimum, maximum }))
}
