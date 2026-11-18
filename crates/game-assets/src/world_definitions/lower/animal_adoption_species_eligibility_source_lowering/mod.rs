use super::source_element_tree_search::{
    authored_type_family_component_attribute, authored_type_family_components,
};
use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    resolved_source_record_index::{BindError, RecordView},
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};
use openzt2_game_data::{
    world_definitions::catalogue_and_progression::animal_adoption_species_eligibility::AnimalAdoptionSpeciesEligibility,
    AssetId,
};

pub(super) fn lower_animal_adoption_species_eligibility(
    record: &RecordView<'_, '_>,
) -> Result<Option<AnimalAdoptionSpeciesEligibility>, BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) || !record.has_type_token("animal")
        || record
            .source_document_element()
            .attribute_named_any(&["abstract"])
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
    {
        return Ok(None);
    }
    if authored_type_family_components(record, "BFAIEntityDataShared").is_empty() {
        return Ok(None);
    }
    let rarity_fame_percent =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "f_adoptRarity")
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value).ok_or_else(|| {
                    BindError::record(record, "invalid concrete animal f_adoptRarity")
                })
            })
            .transpose()?
            .unwrap_or(1.0);
    if !rarity_fame_percent.is_finite() {
        return Err(BindError::record(
            record,
            "non-finite concrete animal f_adoptRarity",
        ));
    }
    if rarity_fame_percent <= 0.0 {
        return Ok(None);
    }
    let Some(species) =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_Species")
    else {
        return Ok(None);
    };
    if species.is_empty() {
        return Err(BindError::record(
            record,
            "concrete animal has an empty s_Species",
        ));
    }
    Ok(Some(AnimalAdoptionSpeciesEligibility {
        id: AssetId::from_key(&canonicalize_source_document_record_key(record.key)),
        species: AssetId::from_key(&canonicalize_source_document_record_key(species)),
        rarity_fame_percent,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        resolved_source_record_index::SourceIndex,
    };

    #[test]
    fn concrete_type_inherits_species_but_keeps_its_own_rarity_override() {
        let parent = parse_blue_fang_source_document(AssetPath::new("entities/animal.xml"), br#"<BFTypedBinder binderType="animal" abstract="true"><types><entity><animal/></entity></types><shared><BFAIEntityDataShared s_Species="ZebraCommon" f_adoptRarity="20"/></shared></BFTypedBinder>"#).unwrap();
        let concrete = parse_blue_fang_source_document(AssetPath::new("entities/zebra.xml"), br#"<BFTypedBinder binderType="ZebraAdult"><types><entity><animal><ZebraAdult/></animal></entity></types><shared><BFAIEntityDataShared f_adoptRarity="1"/></shared></BFTypedBinder>"#).unwrap();
        let index = SourceIndex::build([&parent, &concrete]).unwrap();
        assert_eq!(
            lower_animal_adoption_species_eligibility(&index.find("animal").unwrap()).unwrap(),
            None
        );
        let candidate =
            lower_animal_adoption_species_eligibility(&index.find("zebraadult").unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(candidate.species, AssetId::from_key("zebracommon"));
        assert_eq!(candidate.rarity_fame_percent, 1.0);
        assert_eq!(candidate.id, AssetId::from_key("zebraadult"));
    }
}
