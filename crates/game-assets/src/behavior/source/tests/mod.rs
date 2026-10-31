use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn radial_docking_preserves_target_radius_arrival_radius_and_heading_tolerance() {
    use openzt2_game_data::behavior::action_record::BehaviorAction;
    let source = parse_blue_fang_source_document(
        AssetPath::new("ai/tasks/animals/shelter.beh"),
        br#"<BehaviorSets><Enter><behaviors><BFBehDockRadial subjectNode="Floor" targetRadius="3" rotError="180"/><BFBehDockRadial targetAnim="Stand_Idle" dockRadius="3" targetRadius="10"/></behaviors></Enter></BehaviorSets>"#,
    ).unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets expected");
    };
    let [BehaviorAction::Dock(shelter), BehaviorAction::Dock(view)] =
        sets[0].actions.lowered_actions()
    else {
        panic!("both radial docks expected");
    };
    assert_eq!(shelter.radial_distance_m, Some(3.0));
    assert_eq!(shelter.arrival_radius_m, None);
    assert_eq!(shelter.rotation_tolerance_radians, std::f32::consts::PI);
    assert_eq!(view.radial_distance_m, Some(10.0));
    assert_eq!(view.arrival_radius_m, Some(3.0));
}

#[test]
fn delayed_subject_tokens_preserve_activation_and_expiration_times() {
    use openzt2_game_data::behavior::action_record::BehaviorAction;
    use std::time::Duration;
    let source = parse_blue_fang_source_document(
        AssetPath::new("ai/tasks/animals/token.beh"),
        br#"<BehaviorSets><Feed><behaviors><BFAITokenList><BFAIToken Name="t_Chew" GiveTo="subject" Timein="30" Timeout="180"/></BFAITokenList></behaviors></Feed></BehaviorSets>"#,
    ).unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets expected");
    };
    assert!(!sets[0].actions.has_unsupported_suffix());
    let [BehaviorAction::SendSelfTokens(deliveries)] = sets[0].actions.lowered_actions() else {
        panic!("token delivery expected");
    };
    assert_eq!(deliveries[0].delay, Duration::from_secs(30));
    assert_eq!(deliveries[0].lifetime, Some(Duration::from_secs(180)));
}

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

#[test]
fn spawn_set_preserves_authored_entity_joint_and_clip() {
    use openzt2_game_data::behavior::action_record::BehaviorAction;
    let source = parse_blue_fang_source_document(
        AssetPath::new("spawn.beh"),
        br#"<BehaviorSets><ProduceWaste><behaviors><BFBehSpawn spawnEntity="Waste_Large" subjectNode="WasteSocket" targetAnim="Waste_A"/></behaviors></ProduceWaste></BehaviorSets>"#,
    )
    .unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets expected");
    };
    let [BehaviorAction::SpawnObject {
        entity,
        subject_node,
        animation,
        ..
    }] = sets[0].actions.lowered_actions()
    else {
        panic!("spawn expected");
    };
    assert_eq!(*entity, AssetId::from_key("waste_large"));
    assert_eq!(subject_node, "WasteSocket");
    assert_eq!(animation, "Waste_A");
}

#[test]
fn guest_ground_trash_spawns_beside_the_actor() {
    use openzt2_game_data::behavior::action_record::BehaviorAction;
    // Trash.tsk ThrowAwayTrashGround.
    let source = parse_blue_fang_source_document(
        AssetPath::new("spawn.beh"),
        br#"<BehaviorSets><DropTrash><behaviors><BFBehSpawn spawnEntity="TrashGround" randomPosOffset="0.5" randomRotOffset="180"/></behaviors></DropTrash></BehaviorSets>"#,
    )
    .unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets expected");
    };
    let [BehaviorAction::SpawnObjectAtActor { entity, pose }] = sets[0].actions.lowered_actions()
    else {
        panic!("spawn beside the actor expected");
    };
    assert_eq!(*entity, AssetId::from_key("trashground"));
    assert_eq!(pose.random_position_offset_m, 0.5);
    assert_eq!(pose.random_rotation_offset_radians, 180.0);
}

#[test]
fn keeper_food_placement_lowers_with_its_holding_container_and_placement_set() {
    use openzt2_game_data::behavior::{
        action::{
            locomotion_switch_set::BehaviorLocomotionSwitchSetAction,
            place_target::{BehaviorPlaceTargetAction, BehaviorPlacedTargetType},
        },
        action_record::BehaviorAction,
    };
    let source = parse_blue_fang_source_document(
        AssetPath::new("keeper.beh"),
        br#"<BehaviorSets>
            <Feed><behaviors><ZTBehPlaceTarget container="food" targetType="s_KeeperEatType" targetBeh="PlaceFood"/></behaviors></Feed>
            <PlaceFood><behaviors><BFBehLocoSwitchSet switch="false"><behaviorTable><ground behSet="PlaceFood_Land"/><water behSet="PlaceFood_Water"/><underwater behSet="PlaceFood_Underwater"/></behaviorTable></BFBehLocoSwitchSet></behaviors></PlaceFood>
            <Radius><behaviors><ZTBehPlaceTarget targetType="s_KeeperEatType" locoSpeed="fast" targetAnim="PlaceFood_Idle" targetRadius="2.0"/></behaviors></Radius>
            <Switching><behaviors><BFBehLocoSwitchSet><behaviorTable><ground behSet="Stand"/></behaviorTable></BFBehLocoSwitchSet></behaviors></Switching>
        </BehaviorSets>"#,
    )
    .unwrap();
    let BehaviorDocument::Sets(sets) = lower_behavior_source_document(&source).unwrap() else {
        panic!("sets");
    };
    let [BehaviorAction::PlaceTarget(placement)] = sets[0].actions.lowered_actions() else {
        panic!("placement action");
    };
    assert_eq!(
        *placement,
        BehaviorPlaceTargetAction {
            placed_type: BehaviorPlacedTargetType::KeeperFood,
            holding_container: AssetId::from_key("food"),
            placement_behavior_set: AssetId::from_key("placefood"),
        }
    );
    let [BehaviorAction::LocomotionSwitchSet(switch)] = sets[1].actions.lowered_actions() else {
        panic!("locomotion switch");
    };
    assert_eq!(
        *switch,
        BehaviorLocomotionSwitchSetAction {
            ground: Some(AssetId::from_key("placefood_land")),
            water: Some(AssetId::from_key("placefood_water")),
            underwater: Some(AssetId::from_key("placefood_underwater")),
            follows_medium_changes: false,
        }
    );
    // The direct radius form has no runtime owner.
    assert!(sets[2].actions.has_unsupported_suffix());
    let [BehaviorAction::LocomotionSwitchSet(following)] = sets[3].actions.lowered_actions() else {
        panic!("following locomotion switch");
    };
    assert!(following.follows_medium_changes);
}
