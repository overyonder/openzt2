use super::*;
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn attachment_preserves_clip_spelling_and_does_not_invent_a_default_detach_rule() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("ai/tasks/staff/attach.beh"),
        br#"<BehaviorSets><Carry><behaviors><BFBehAttachObject attachEntity="Bucket" targetAnim="Stand_2StandItem"/></behaviors></Carry></BehaviorSets>"#,
    ).unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets expected");
    };
    let [openzt2_game_data::behavior::action_record::BehaviorAction::AttachObject(action)] =
        sets[0].actions.lowered_actions()
    else {
        panic!("attachment expected");
    };
    assert_eq!(action.entity, AssetId::from_key("bucket"));
    assert_eq!(action.animation.as_deref(), Some("Stand_2StandItem"));
    assert_eq!(action.detach_rule, AssetId::default());
}

#[test]
fn task_evaluation_retains_numeric_attribute_values_separately_from_policy_and_outcomes() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("ai/tasks/animals/evaluation.tsk"),
        br#"<BFAITaskTemplateList><BFAITaskTemplate Name="Wander" UniqueID="test:Wander">
            <BFAICreateData><Subjects><Animal/></Subjects><Targets><self/></Targets><Objects/></BFAICreateData>
            <BFAIEvalData fixedScore="6" distanceInfluenced="false"><BFAIAttributeFloatMap exercise="-30" f_needPointsGood="12"/></BFAIEvalData>
            <BFBehExecTask/>
            <BFAICompletionData><BFBehExecTask><BFBehPlaySet behSet="Stand"/></BFBehExecTask><BFAIAttributeFloatMap exercise="-50"/></BFAICompletionData>
            <BFAIFailureData><BFAIAttributeFloatMap hunger="10"/></BFAIFailureData>
        </BFAITaskTemplate></BFAITaskTemplateList>"#,
    ).unwrap();
    let BehaviorDocument::Tasks(tasks) = lower_behavior_source_document(&source).unwrap() else {
        panic!("expected tasks");
    };
    use openzt2_game_data::behavior::score::BehaviorScore;
    let [BehaviorScore::Fixed(fixed), BehaviorScore::DistanceInfluence(false), BehaviorScore::AttributeValue {
        attribute: exercise,
        value: delta,
    }, BehaviorScore::AttributeValue {
        attribute: good_needs,
        value: good_delta,
    }] = tasks[0].scores.as_slice()
    else {
        panic!("evaluation policy and numeric map entries must retain separate identities");
    };
    assert_eq!(fixed.to_bits(), 6.0_f32.to_bits());
    assert_eq!(*exercise, AssetId::from_key("exercise"));
    assert_eq!(
        delta
            .sample_q16(|_| unreachable!())
            .expect("context-free scalar"),
        -30 * 65536
    );
    assert_eq!(*good_needs, AssetId::from_key("f_needPointsGood"));
    assert_eq!(
        good_delta
            .sample_q16(|_| unreachable!())
            .expect("context-free scalar"),
        12 * 65536
    );
    use openzt2_game_data::behavior::{
        action::{entity_role::BehaviorEntityRole, modification::BehaviorFact},
        action_record::BehaviorAction,
    };
    let [BehaviorAction::FactModifications(completion), BehaviorAction::PlaySet(continuation)] =
        tasks[0].completion.lowered_actions()
    else {
        panic!("terminal need changes must precede the completion continuation");
    };
    assert!(!tasks[0].completion.has_unsupported_suffix());
    assert_eq!(completion.len(), 1);
    assert_eq!(
        completion[0].affected_entity_role,
        BehaviorEntityRole::Subject
    );
    assert_eq!(completion[0].modified_fact, BehaviorFact::Exercise);
    assert_eq!(
        completion[0]
            .modification_value
            .sample_q16(|_| unreachable!())
            .expect("context-free scalar"),
        -50 * 65536
    );
    assert_eq!(
        continuation.behavior_set_asset_id,
        AssetId::from_key("stand")
    );
    let [BehaviorAction::FactModifications(failure)] = tasks[0].failure.lowered_actions() else {
        panic!("failure effects without an executable must be retained");
    };
    assert_eq!(failure[0].modified_fact, BehaviorFact::Hunger);
    assert_eq!(
        failure[0]
            .modification_value
            .sample_q16(|_| unreachable!())
            .expect("context-free scalar"),
        10 * 65536
    );
}

#[test]
fn unsupported_set_suffix_does_not_discard_other_declarations() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("ai/tasks/staff/test.beh"),
        br#"<BehaviorSets>
            <PartlyMapped><behaviors><BFBehPlaySet behSet="Idle"/><UnmappedAction/><BFBehPlaySet behSet="MustNotRun"/></behaviors></PartlyMapped>
            <Idle><behaviors><BFBehPlaySet behSet="Stand"/></behaviors></Idle>
        </BehaviorSets>"#,
    ).unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("expected behavior sets");
    };
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].actions.lowered_actions().len(), 1);
    assert!(sets[0].actions.has_unsupported_suffix());
    assert_eq!(sets[1].actions.lowered_actions().len(), 1);
    assert!(!sets[1].actions.has_unsupported_suffix());
}
