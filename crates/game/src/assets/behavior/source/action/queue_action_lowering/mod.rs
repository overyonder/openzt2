//! Lower authored queue waiting policy; loaded snapshots use the runtime owner.

use super::{invalid_behavior_source_data, source_value_reading::read_optional_float_attribute};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{
    behavior::{action::queue::BehaviorQueueWaitAction, action_record::BehaviorAction},
    AssetId,
};
use std::{io, time::Duration};

pub(super) fn lower_queue_wait_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehWaitQueue" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "container" | "waitBehSet" | "minWaitTime" | "maxWaitTime"
        )
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehWaitQueue has unmapped context or saved runtime state",
        ));
    }
    let duration = |name| -> io::Result<Duration> {
        let seconds = read_optional_float_attribute(node, name)?.unwrap_or(0.0);
        Duration::try_from_secs_f64(f64::from(seconds)).map_err(|_| {
            invalid_behavior_source_data("queue wait duration must be finite and nonnegative")
        })
    };
    let minimum_wait = duration("minWaitTime")?;
    let maximum_wait = duration("maxWaitTime")?;
    if maximum_wait < minimum_wait {
        return Err(invalid_behavior_source_data(
            "queue wait maximum is below minimum",
        ));
    }
    let identifier = |name| {
        node.attribute(name)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
    };
    Ok(Some(BehaviorAction::WaitInteractionQueue(
        BehaviorQueueWaitAction {
            container: identifier("container"),
            waiting_behavior_set: identifier("waitBehSet").unwrap_or_default(),
            minimum_wait,
            maximum_wait,
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn queue_wait_retains_container_waiting_set_and_seconds() {
        let source = parse_blue_fang_source_document(AssetPath::new("queue.beh"),
            br#"<behaviors><BFBehWaitQueue container="Food" waitBehSet="GuestWait" minWaitTime="3.5" maxWaitTime="12"/></behaviors>"#).expect("source");
        let action =
            lower_queue_wait_action(source.root.element_children().next().expect("action"))
                .expect("lower")
                .expect("queue");
        let BehaviorAction::WaitInteractionQueue(wait) = action else {
            panic!("queue action")
        };
        assert_eq!(wait.container, Some(AssetId::from_key("food")));
        assert_eq!(wait.waiting_behavior_set, AssetId::from_key("guestwait"));
        assert_eq!(wait.minimum_wait, Duration::from_millis(3500));
        assert_eq!(wait.maximum_wait, Duration::from_secs(12));
    }
}
