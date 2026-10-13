use std::io;

use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::invalid_behavior_source_data;

pub(super) fn lower_trick_learning_action(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if !matches!(
        node.name.as_str(),
        "BFBehSetAttribute" | "BFAISubjectData" | "BFAITargetData"
    ) {
        return Ok(None);
    }
    let owners: Vec<_> = if node.name == "BFBehSetAttribute" {
        node.element_children().collect()
    } else {
        vec![node]
    };
    if !owners.iter().any(|owner| {
        owner
            .element_children()
            .any(|child| child.name == "TrickLearning")
    }) {
        return Ok(None);
    }
    // Mixed attribute writes cannot be partially applied as a learning action.
    let [owner] = owners.as_slice() else {
        return Err(invalid_behavior_source_data("mixed trick learning owners"));
    };
    if !node.attributes.is_empty()
        || owner.name != "BFAISubjectData"
        || !owner.attributes.is_empty()
    {
        return Err(invalid_behavior_source_data(
            "unsupported trick learning owner",
        ));
    }
    let children: Vec<_> = owner.element_children().collect();
    let [learning] = children.as_slice() else {
        return Err(invalid_behavior_source_data("mixed trick learning effects"));
    };
    let tricks: Vec<_> = learning.element_children().collect();
    let [trick] = tricks.as_slice() else {
        return Err(invalid_behavior_source_data(
            "expected one trick learning grant",
        ));
    };
    if !learning.attributes.is_empty()
        || trick.element_children().next().is_some()
        || trick
            .attributes
            .iter()
            .any(|attribute| !matches!(attribute.name(), "learn" | "cap"))
    {
        return Err(invalid_behavior_source_data(
            "unsupported trick learning policy",
        ));
    }
    let number = |name| {
        trick
            .attribute(name)
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or_else(|| {
                invalid_behavior_source_data("trick learning requires unsigned learn and cap")
            })
    };
    Ok(Some(BehaviorAction::LearnTrick {
        // Trick definitions retain authored spelling, unlike behavior-set names.
        trick: AssetId::from_key(&trick.name),
        amount: number("learn")?,
        cap: number("cap")?,
    }))
}
