use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    source_document::{
        authored_behavior_type_identifiers::authored_behavior_type_identifiers,
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        resolved_source_record_index::SourceIndex,
    },
};
use openzt2_game_data::{behavior::eligibility::BehaviorCandidateRole, AssetId};

use super::candidate_requirements_are_satisfied;

#[test]
fn inherited_food_classifications_and_variant_overrides_drive_authored_task_compatibility() {
    let parent = parse_blue_fang_source_document(AssetPath::new("entities/parent.xml"), br#"<BFTypedBinder binderType="Parent"><shared><BFAIEntityDataShared b_Carnivore="true" b_FoodDishEater="true"/></shared></BFTypedBinder>"#).unwrap();
    let child = parse_blue_fang_source_document(AssetPath::new("entities/child.xml"), br#"<BFTypedBinder binderType="Child"><types><Parent><Child/></Parent></types><shared><BFAIEntityDataShared b_Carnivore="false" b_Herbivore="true"/></shared></BFTypedBinder>"#).unwrap();
    let index = SourceIndex::build([&parent, &child]).unwrap();
    let parent_types = authored_behavior_type_identifiers(&index.find("Parent").unwrap());
    let child_types = authored_behavior_type_identifiers(&index.find("Child").unwrap());
    for (diet, source, parent_accepts, child_accepts) in [
        ("<b_Carnivore/>", "Meat", true, false),
        ("<b_Herbivore/>", "Grass", false, true),
        ("", "Water", true, true),
        ("<b_Piscivore/>", "Fish", false, false),
    ] {
        let asset = BehaviorDocumentAsset::from_test_source(format!(r#"<BFAITaskTemplateList><BFAITaskTemplate Name="Consume" UniqueID="consume"><BFAICreateData><Subjects_AND>{diet}<b_FoodDishEater/></Subjects_AND><Targets><{source}/></Targets></BFAICreateData><BFBehExecTask/></BFAITaskTemplate></BFAITaskTemplateList>"#).as_bytes());
        let task = asset.behavior_task_at_index(0).unwrap();
        for (types, expected) in [
            (&parent_types, parent_accepts),
            (&child_types, child_accepts),
        ] {
            let accepted = candidate_requirements_are_satisfied(
                &task.candidate_eligibility_requirements,
                |role, required| match role {
                    BehaviorCandidateRole::Subject => types.contains(&required),
                    BehaviorCandidateRole::Target => {
                        required == AssetId::from_key(&source.to_ascii_lowercase())
                    }
                    BehaviorCandidateRole::Object => false,
                },
                |_, _| false,
            );
            assert_eq!(accepted, expected, "{source}");
        }
    }
}
