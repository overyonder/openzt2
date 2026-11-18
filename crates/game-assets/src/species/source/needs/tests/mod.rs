use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    resolved_source_record_index::SourceIndex,
};

#[test]
fn inherited_biome_sensitivities_preserve_sign_and_override_by_biome() {
    let parent = parse_blue_fang_source_document(
        AssetPath::new("entities/parent.xml"),
        br#"<BFTypedBinder binderType="Parent"><shared>
            <BFAIStateVar Name="environment" Value="25"/>
            <BFGBiomeData><tundra sensitivity="10"/><grassland sensitivity="0"/><desert sensitivity="-10"/></BFGBiomeData>
        </shared></BFTypedBinder>"#,
    ).unwrap();
    let child = parse_blue_fang_source_document(
        AssetPath::new("entities/child.xml"),
        br#"<BFTypedBinder binderType="Child"><types><Parent><Child/></Parent></types>
            <shared><BFGBiomeData><grassland sensitivity="2"/></BFGBiomeData></shared>
        </BFTypedBinder>"#,
    )
    .unwrap();
    let index = SourceIndex::build([&parent, &child]).unwrap();
    let need = lower_effective_authored_animal_need_to_canonical_species_need(
        &index.find("Child").unwrap(),
        NeedKind::Environment,
        "environment",
    )
    .unwrap();
    assert_eq!(need.initial_wellness_permille_range, [750; 2]);
    assert_eq!(need.preferences.len(), 3);
    for (biome, sensitivity) in [("tundra", 10), ("grassland", 2), ("desert", -10)] {
        assert_eq!(
            need.preferences
                .iter()
                .find(|row| row.target == AssetId::from_key(biome))
                .unwrap()
                .weight,
            sensitivity
        );
    }
}

#[test]
fn reproduction_is_lowered_when_authored_and_optional_otherwise() {
    let states = [
        "hunger",
        "thirst",
        "rest",
        "privacy",
        "social",
        "exercise",
        "stimulation",
        "environment",
        "health",
        "hygiene",
        "bathroom",
    ]
    .map(|name| format!(r#"<BFAIStateVar Name="{name}" Value="10"/>"#))
    .concat();
    let lower = |extra: &str| {
        let source = format!(
            r#"<BFTypedBinder binderType="Animal"><shared>{states}{extra}</shared></BFTypedBinder>"#
        );
        let document = parse_blue_fang_source_document(
            AssetPath::new("entities/animal.xml"),
            source.as_bytes(),
        )
        .unwrap();
        let index = SourceIndex::build([&document]).unwrap();
        lower_effective_authored_species_need_facts(&index.find("Animal").unwrap()).unwrap()
    };
    let without = lower("");
    assert_eq!(without.len(), 11);
    let with = lower(
        r#"<BFAIStateVar Name="reproduction" ValueMin="10" ValueMax="20" TriggerThreshold="50"/><BFAINeedAdjusts reproduction="0.25"/>"#,
    );
    let reproduction = with
        .iter()
        .find(|need| need.kind == NeedKind::Reproduction)
        .expect("reproduction need");
    assert_eq!(reproduction.initial_wellness_permille_range, [800, 900]);
    assert_eq!(reproduction.trigger_wellness_threshold, Some(500));
    assert!(reproduction.wellness_adjustment_per_update_q16 < 0);
}
