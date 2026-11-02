use crate::source_document::{
    resolved_source_record_index::RecordView,
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};
use openzt2_game_data::{
    world_definitions::entity_naming::{EntityNamingDefinition, GeneratedEntityNameDefinition},
    AssetId,
};

use super::source_element_tree_search::authored_type_family_component_attribute;

fn append_counted_tags(record: &RecordView<'_, '_>, tags: &mut Vec<AssetId>) {
    tags.extend(
        naming_type_chain(record)
            .into_iter()
            .rev()
            .take_while(|tag| tag != "entity")
            .map(|tag| AssetId::from_key(&tag)),
    );
}

fn naming_type_chain(record: &RecordView<'_, '_>) -> Vec<String> {
    let mut types = record.type_tokens();
    if record
        .source_document_element()
        .attribute_named_any(&["extends", "base", "parentType", "inherit"])
        .is_some()
    {
        let concrete = canonicalize_source_document_record_key(record.key);
        if !types.contains(&concrete) {
            types.push(concrete);
        }
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
        resolved_source_record_index::SourceIndex,
    };

    #[test]
    fn explicit_inheritance_retains_the_derived_counter_and_direct_base_name() {
        let base = parse_blue_fang_source_document(AssetPath::new("entities/entrance.xml"), br#"<BFTypedBinder binderType="entrance"><types><entity><building><entrance/></building></entity></types><shared><BFAIEntityDataShared s_GenerateName="byType"/></shared></BFTypedBinder>"#).unwrap();
        let derived = parse_blue_fang_source_document(
            AssetPath::new("entities/frontgate.xml"),
            br#"<BFTypedBinder binderType="frontgate" extends="entrance"/>"#,
        )
        .unwrap();
        let base_named = parse_blue_fang_source_document(AssetPath::new("entities/staffgate.xml"), br#"<BFTypedBinder binderType="staffgate" extends="entrance"><shared><BFAIEntityDataShared s_GenerateName="byBaseType"/></shared></BFTypedBinder>"#).unwrap();
        let index = SourceIndex::build([&base, &derived, &base_named]).unwrap();
        let derived = lower_entity_naming(&index.find("frontgate").unwrap()).unwrap();
        assert_eq!(
            derived.counted_tags,
            ["frontgate", "entrance", "building"].map(AssetId::from_key)
        );
        assert_eq!(
            derived.generated_name.unwrap().counter_tag,
            AssetId::from_key("frontgate")
        );
        let base_named = lower_entity_naming(&index.find("staffgate").unwrap()).unwrap();
        assert_eq!(
            base_named.generated_name.unwrap().counter_tag,
            AssetId::from_key("entrance")
        );
    }

    #[test]
    fn species_naming_reuses_ancestor_counter_without_counting_entity_or_flags() {
        let animal = parse_blue_fang_source_document(AssetPath::new("entities/animal.xml"), br#"<BFTypedBinder binderType="animal" abstract="true"><types><entity><actor><animal/></actor></entity></types><shared><BFAIEntityDataShared s_GenerateName="bySpecies" b_Dirty="true"/></shared></BFTypedBinder>"#).unwrap();
        let species = parse_blue_fang_source_document(AssetPath::new("entities/zebra.xml"), br#"<BFTypedBinder binderType="zebra" abstract="true"><types><entity><actor><animal><zebra/></animal></actor></entity></types><shared><BFAIEntityDataShared s_Species="zebra"/></shared></BFTypedBinder>"#).unwrap();
        let adult = parse_blue_fang_source_document(AssetPath::new("entities/zebraadult.xml"), br#"<BFTypedBinder binderType="zebraadult"><types><entity><actor><animal><zebra><zebraadult/></zebra></animal></actor></entity></types></BFTypedBinder>"#).unwrap();
        let hybrid = parse_blue_fang_source_document(AssetPath::new("entities/hybrid.xml"), br#"<BFTypedBinder binderType="hybrid"><types><entity><actor><animal><hybrid/></animal></actor></entity></types><shared><BFAIEntityDataShared s_Species="zebra"/></shared></BFTypedBinder>"#).unwrap();
        let index = SourceIndex::build([&animal, &species, &adult, &hybrid]).unwrap();
        let naming = lower_entity_naming(&index.find("zebraadult").unwrap()).unwrap();
        assert_eq!(
            naming.counted_tags,
            ["zebraadult", "zebra", "animal", "actor"].map(AssetId::from_key)
        );
        assert_eq!(
            naming.generated_name,
            Some(GeneratedEntityNameDefinition {
                localization_key: AssetId::from_key("entityname:zebra"),
                counter_tag: AssetId::from_key("zebra")
            })
        );
        let hybrid = lower_entity_naming(&index.find("hybrid").unwrap()).unwrap();
        assert_eq!(
            hybrid.counted_tags,
            ["hybrid", "animal", "actor", "zebra", "animal", "actor"].map(AssetId::from_key)
        );
    }
}

pub(super) fn lower_entity_naming(record: &RecordView<'_, '_>) -> Option<EntityNamingDefinition> {
    if !source_document_names_are_semantically_equal(
        &record.source_document_element().name,
        "BFTypedBinder",
    ) || !record.has_type_token("entity")
    {
        return None;
    }
    let types = naming_type_chain(record);
    let mut counted_tags = Vec::new();
    append_counted_tags(record, &mut counted_tags);
    let selector =
        authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_GenerateName");
    let name_type = match selector
        .map(canonicalize_source_document_record_key)
        .as_deref()
    {
        Some("bytype") => types.last().cloned(),
        Some("bybasetype") => types
            .len()
            .checked_sub(2)
            .and_then(|index| types.get(index))
            .cloned(),
        Some("byspecies") => {
            authored_type_family_component_attribute(record, "BFAIEntityDataShared", "s_Species")
                .and_then(|species| record.find_resolved_source_record_by_reference(species))
                .and_then(|species| {
                    let name_type = naming_type_chain(&species).last().cloned()?;
                    if !types.contains(&name_type) {
                        append_counted_tags(&species, &mut counted_tags);
                    }
                    Some(name_type)
                })
        }
        _ => None,
    };
    Some(EntityNamingDefinition {
        id: AssetId::from_key(&canonicalize_source_document_record_key(record.key)),
        counted_tags,
        generated_name: name_type.map(|tag| GeneratedEntityNameDefinition {
            localization_key: AssetId::from_key(&format!("entityname:{tag}")),
            counter_tag: AssetId::from_key(&tag),
        }),
    })
}
