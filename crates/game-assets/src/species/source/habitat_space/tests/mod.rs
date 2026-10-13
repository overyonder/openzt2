use super::*;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    resolved_source_record_index::SourceIndex,
};

#[test]
fn species_loading_preserves_inherited_space_and_specific_overrides() {
    let needs = [
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
    .map(|name| format!(r#"<BFAIStateVar Name="{name}" Value="0"/>"#))
    .join("");
    let parent = parse_blue_fang_source_document(
        AssetPath::new("entities/parent.xml"),
        format!(
            r#"<BFTypedBinder binderType="Parent"><shared>
            <BFAIEntityDataShared s_Species="Parent" f_RequiredInitialSpace="120" f_RequiredAdditionalSpace="30"/>
            {needs}</shared></BFTypedBinder>"#
        )
        .as_bytes(),
    )
    .unwrap();
    let child = parse_blue_fang_source_document(
        AssetPath::new("entities/child.xml"),
        br#"<BFTypedBinder binderType="Child"><types><Parent><Child/></Parent></types>
            <shared><BFAIEntityDataShared f_RequiredInitialSpace="240"/></shared></BFTypedBinder>"#,
    )
    .unwrap();
    let index = SourceIndex::build([&parent, &child]).unwrap();
    let species = super::super::lower_species_source_record_to_canonical_species(
        &index.find("Child").unwrap(),
    )
    .unwrap();
    assert_eq!(
        species.habitat_space,
        Some(SpeciesHabitatSpace {
            initial_square_metres: NonNegativeSquareMetres::from_finite_non_negative(240.0)
                .unwrap(),
            additional_square_metres: NonNegativeSquareMetres::from_finite_non_negative(30.0)
                .unwrap(),
        })
    );
    assert_eq!(species.needs.len(), 11);
}
