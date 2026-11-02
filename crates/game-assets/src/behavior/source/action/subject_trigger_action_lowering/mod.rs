use std::io;

use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::invalid_behavior_source_data;

pub(super) fn lower_subject_trigger(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    if node.name != "BFBehSendTrigger" {
        return Ok(None);
    }
    if node
        .attributes
        .iter()
        .any(|attribute| attribute.name() != "subjectTrigger")
        || node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "unsupported trigger recipient or payload",
        ));
    }
    let trigger = node
        .attribute("subjectTrigger")
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid_behavior_source_data("subject trigger requires its name"))?;
    Ok(Some(BehaviorAction::SendSubjectTrigger(AssetId::from_key(
        &trigger.to_ascii_lowercase(),
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        behavior::source::lower_behavior_source_document,
        source_document::{
            blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        },
    };
    use openzt2_game_data::behavior::document::BehaviorDocument;

    #[test]
    fn subject_survey_and_fame_triggers_stay_before_actor_removal() {
        let source = parse_blue_fang_source_document(
            AssetPath::new("ai/tasks/guests/leave.beh"),
            br#"<BehaviorSets><Leave><behaviors><BFBehSendTrigger subjectTrigger="Adjust_SurveyData"/><BFBehSendTrigger subjectTrigger="Adjust_ZooFame"/><BFBehKill killSubject="true"/></behaviors></Leave></BehaviorSets>"#,
        ).unwrap();
        let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
            panic!("sets expected");
        };
        let [BehaviorAction::SendSubjectTrigger(survey), BehaviorAction::SendSubjectTrigger(fame), BehaviorAction::Termination(kill)] =
            sets[0].actions.lowered_actions()
        else {
            panic!("trigger order must remain authored");
        };
        assert_eq!(*survey, AssetId::from_key("adjust_surveydata"));
        assert_eq!(*fame, AssetId::from_key("adjust_zoofame"));
        assert!(kill.kill_subject);
    }
}
