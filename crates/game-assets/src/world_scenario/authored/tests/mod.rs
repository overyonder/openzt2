use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[test]
fn nested_group_completion_keeps_both_parallel_rules_and_its_callback() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("scenario/goals/startup.xml"),
        br#"<ZTScenarioMgr><children><BFScenarioGroup name="startup" allsuccess="true"><children>
          <BFScenarioGroup name="animals" allsuccess="true" chain="false">
            <success><BFScenarioScriptAction script="startup.lua" entry="animalsready"/></success>
            <children><BFScenarioRule name="habitat"/><BFScenarioRule name="food"/></children>
          </BFScenarioGroup><BFScenarioRule name="stands"/>
        </children></BFScenarioGroup></children></ZTScenarioMgr>"#,
    )
    .expect("valid scenario source");
    let document = lower_documents(&[&source], &[], &BTreeMap::new(), None).unwrap();
    let scenario = &document.scenarios[0];
    let animals = &scenario.objective_groups[0];
    assert_eq!(
        animals.members,
        [scenario.objectives[0].id, scenario.objectives[1].id]
    );
    assert_eq!(
        scenario.objective_groups[1].members,
        [animals.id, scenario.objectives[2].id]
    );
    let callback = document
        .scenario_script_bindings
        .iter()
        .find(|binding| binding.entry == "animalsready")
        .unwrap();
    assert_eq!(callback.objective, animals.id);
    assert_eq!(callback.phase, ScenarioScriptPhase::Success);
}

#[test]
fn scenario_briefing_and_objectives_use_canonical_localization_keys() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("scenario/goals/briefing.xml"),
        br#"<ZTScenarioMgr><info locid="Example:Introduction"/><children>
            <BFScenarioRule name="goal"><info type="primary">
                <neutral locid="Example:Goal"/>
                <success locid="Example:GoalOK"/>
                <failure locid="Example:GoalFailed"/>
            </info></BFScenarioRule>
        </children></ZTScenarioMgr>"#,
    )
    .expect("valid scenario source");
    let document =
        lower_documents(&[&source], &[], &BTreeMap::new(), None).expect("valid scenario");
    let scenario = &document.scenarios[0];
    assert_eq!(
        scenario.description_key,
        AssetId::from_key("example:introduction")
    );
    assert_eq!(
        scenario.objectives[0].text_key,
        AssetId::from_key("example:goal")
    );
    assert_eq!(
        scenario.objectives[0].success_text_key,
        AssetId::from_key("example:goalok")
    );
    assert_eq!(
        scenario.objectives[0].failure_text_key,
        AssetId::from_key("example:goalfailed")
    );
}

#[test]
fn scenario_overview_uses_group_instructions_before_manager_title() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("scenario/goals/briefing.xml"),
        br#"<ZTScenarioMgr><info locid="Example:Title"/><children>
            <BFScenarioGroup><info type="primary"><neutral overview="Example:Instructions"/></info>
                <children><BFScenarioRule name="goal"/></children>
            </BFScenarioGroup>
        </children></ZTScenarioMgr>"#,
    )
    .expect("valid scenario source");
    let document =
        lower_documents(&[&source], &[], &BTreeMap::new(), None).expect("valid scenario");
    assert_eq!(
        document.scenarios[0].description_key,
        AssetId::from_key("example:instructions")
    );
}

#[test]
fn named_hidden_rules_keep_their_script_identity_and_chain() {
    let source = parse_blue_fang_source_document(
        AssetPath::new("scenario/goals/challenge/example.xml"),
        br#"<ZTScenarioMgr><children><BFScenarioGroup chain="true"><children>
            <BFScenarioRule name="offer" hidden="1">
                <evaluate><BFScenarioScriptAction script="example.lua" entry="offer"/></evaluate>
            </BFScenarioRule>
            <BFScenarioRule name="photogoal" hidden="1"><info type="photo"/></BFScenarioRule>
            <BFScenarioRule/>
        </children></BFScenarioGroup></children></ZTScenarioMgr>"#,
    )
    .expect("valid scenario source");
    let document =
        lower_documents(&[&source], &[], &BTreeMap::new(), None).expect("valid scenario");
    let rules = &document.scenarios[0].objectives;
    assert_eq!(rules[0].name, Some(AssetId::from_key("offer")));
    assert_eq!(rules[1].name, Some(AssetId::from_key("photogoal")));
    assert_eq!(rules[2].name, None);
    assert!(rules[0].hidden && rules[1].hidden);
    assert_eq!(rules[1].prerequisite_objectives, vec![rules[0].id]);
    assert_eq!(document.scenario_script_bindings[0].objective, rules[0].id);
    assert_ne!(rules[0].id, rules[0].name.expect("named rule"));
}

#[test]
fn children_wrapper_preserves_manager_sequencing_between_groups() {
    for chain in [false, true] {
        let source = format!(
            r#"<ZTScenarioMgr chain="{chain}"><children>
                <BFScenarioGroup chain="true"><children>
                    <BFScenarioRule name="welcome"/>
                    <BFScenarioRule name="completion"/>
                </children></BFScenarioGroup>
                <BFScenarioGroup chain="true"><children>
                    <BFScenarioRule name="interaction"/>
                    <BFScenarioRule name="completion"/>
                </children></BFScenarioGroup>
            </children></ZTScenarioMgr>"#,
        );
        let source = parse_blue_fang_source_document(
            AssetPath::new("scenario/goals/tutorial_sequence.xml"),
            source.as_bytes(),
        )
        .expect("valid scenario source");
        let document =
            lower_documents(&[&source], &[], &BTreeMap::new(), None).expect("valid scenario");
        let rules = &document.scenarios[0].objectives;
        assert!(rules[0].prerequisite_objectives.is_empty());
        assert_eq!(rules[1].prerequisite_objectives, [rules[0].id]);
        assert_eq!(
            rules[2].prerequisite_objectives,
            if chain { vec![rules[1].id] } else { vec![] }
        );
        assert_eq!(rules[3].prerequisite_objectives, [rules[2].id]);
        assert_eq!(rules[1].name, rules[3].name);
        assert_ne!(rules[1].id, rules[3].id);
    }
}
