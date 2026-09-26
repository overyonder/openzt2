use super::{invalid_behavior_source_data, source_value_reading::read_boolean_attribute_or_false};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::{
    behavior::{
        action::synchronized_sets::BehaviorSynchronizedSetsAction, action_record::BehaviorAction,
    },
    AssetId,
};
use std::io;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn shop_pair_retains_both_sets_and_phase_reset() {
        let source = parse_blue_fang_source_document(AssetPath::new("hotdog.beh"), br#"<behaviors><BFBehSyncSet subjectBehSet="GuestBuyHotdog" targetBehSet="BuildingSellHotdog" resetPhase="true"/></behaviors>"#).expect("source");
        let action =
            lower_synchronized_sets(source.root.element_children().next().expect("action"))
                .expect("lower")
                .expect("pair");
        let BehaviorAction::SynchronizedSets(pair) = action else {
            panic!("pair")
        };
        assert_eq!(
            pair.subject_behavior_set,
            AssetId::from_key("guestbuyhotdog")
        );
        assert_eq!(
            pair.target_behavior_set,
            AssetId::from_key("buildingsellhotdog")
        );
        assert!(pair.reset_animation_phase);
    }
}

pub(super) fn lower_synchronized_sets(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehSyncSet" {
        return Ok(None);
    }
    if node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "subjectBehSet" | "targetBehSet" | "resetPhase"
        )
    }) || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehSyncSet has unmapped entity override or nested task context",
        ));
    }
    let identifier = |name| {
        node.attribute(name)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| AssetId::from_key(&value.to_ascii_lowercase()))
            .ok_or_else(|| invalid_behavior_source_data(format!("BFBehSyncSet has no {name}")))
    };
    Ok(Some(BehaviorAction::SynchronizedSets(
        BehaviorSynchronizedSetsAction {
            subject_behavior_set: identifier("subjectBehSet")?,
            target_behavior_set: identifier("targetBehSet")?,
            reset_animation_phase: read_boolean_attribute_or_false(node, "resetPhase")?,
        },
    )))
}
