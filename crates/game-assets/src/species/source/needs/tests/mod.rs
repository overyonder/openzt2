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
