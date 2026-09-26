use super::catalogue_category_source_vocabulary::catalogue_category;
use super::source_element_tree_search::authored_type_family_elements_named;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_flag_vocabulary::catalogue_flag;
use super::world_definition_source_value_reading_and_conversion::{asset, flags, id, required};
#[cfg(test)]
use crate::assets::source_document::resolved_source_record_index::SourceIndex;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::{
    CatalogueCategory, CatalogueEntry, CatalogueFilterFlags, CatalogueFilterValue,
};
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

pub(super) fn authored_catalogue_filter_values(
    record: &RecordView<'_, '_>,
) -> Vec<openzt2_game_data::world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueFilterValue>{
    let mut values = BTreeMap::new();
    for component in authored_type_family_elements_named(record, "BFAIEntityDataShared") {
        for (field, value) in component.attributes() {
            // Native TypeListFilters selects text and boolean shared-data
            // fields. Keep the winning inherited value, including empty masks.
            if field.get(..2).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case("s_") || prefix.eq_ignore_ascii_case("b_")
            }) || field.eq_ignore_ascii_case("biome")
            {
                values
                    .entry(field.to_ascii_lowercase())
                    .or_insert_with(|| CatalogueFilterValue {
                        field: field.to_ascii_lowercase(),
                        value: value.to_owned(),
                    });
            }
        }
    }
    values
        .into_values()
        .filter(|value| !value.value.is_empty())
        .collect()
}

pub(super) fn authored_catalogue_purchase_sort_key(record: &RecordView<'_, '_>) -> String {
    authored_type_family_elements_named(record, "BFAIEntityDataShared")
        .into_iter()
        .find_map(|shared_data| shared_data.attribute_named_any(&["s_uisort"]))
        .or_else(|| record.value(&["s_uisort", "uiSort", "sortOrder", "order"]))
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn authored_catalogue_purchase_sort_fallback_type_name(
    record: &RecordView<'_, '_>,
) -> String {
    record
        .source_document_element()
        .attribute_named_any(&["binderType", "typeName", "entityName", "id", "key"])
        .unwrap_or(record.key)
        .to_owned()
}

pub(super) fn bind_catalogue(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let kind = asset(record, &["kind", "type"]);
    let kinds = vec![kind];
    let filter_flag_bits = flags(record.value(&["filters", "flags"]), catalogue_flag)? as u16;
    let filters = CatalogueFilterFlags::from_raw_flag_bits(filter_flag_bits).ok_or_else(|| {
        BindError::record(
            record,
            format!("invalid catalogue filter flags {filter_flag_bits:#x}"),
        )
    })?;
    output.document.catalogue.push(CatalogueEntry {
        filter_values: authored_catalogue_filter_values(record),
        id: id(record.key),
        definition: asset(record, &["definition", "subject"]),
        kind,
        kinds,
        category: catalogue_category(required(record, &["category"])?)?,
        authored_purchase_sort_key: authored_catalogue_purchase_sort_key(record),
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters,
        name_key: asset(record, &["nameKey", "displayName"]),
        icon: asset(record, &["icon"]),
    });
    Ok(())
}

/// Lowers the authored concrete elevated-curb purchase rows consumed by the
/// shell's `ZT_AUTOPOPULATE_LIST` type list. The source binder/tree exists only
/// during lowering; the game receives one final catalogue row per selectable curb.
pub(super) fn bind_authored_type_list_entry(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "BFTypedBinder",
    ) || !record.has_type_token("elevatedcurb")
        || record
            .source_document_element()
            .attribute_named_any(&["abstract"])
            .is_some_and(|value| {
                matches!(
                    canonicalize_source_document_record_key(value).as_str(),
                    "true" | "1"
                )
            })
    {
        return Ok(());
    }
    let Some(button) = record.descendant_named("UIToggleButton") else {
        return Ok(());
    };
    let Some(image) = record
        .descendant_named("default")
        .and_then(|element| element.attribute_named_any(&["image"]))
    else {
        return Err(BindError::record(
            record,
            "elevated-curb purchase row has no default icon",
        ));
    };
    let name_key = record
        .descendant_named("UIHelpInfo")
        .and_then(|element| element.attribute_named_any(&["ids"]))
        .map(id)
        .unwrap_or_default();
    let template = button
        .attribute_named_any(&["template"])
        .unwrap_or_default();
    if !source_document_names_are_semantically_equal(template, "elevatedcurbplacement") {
        return Err(BindError::record(
            record,
            format!("elevated-curb purchase row uses unsupported template {template:?}"),
        ));
    }
    let kind = AssetId::from_key("elevatedcurb");
    let kinds = vec![kind];
    output.document.catalogue.push(CatalogueEntry {
        id: id(&format!("catalogue/elevatedcurb/{}", record.key)),
        filter_values: authored_catalogue_filter_values(record),
        definition: id(record.key),
        kind,
        kinds,
        category: CatalogueCategory::Paths,
        authored_purchase_sort_key: String::new(),
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters: CatalogueFilterFlags::BUILDABLE,
        name_key,
        icon: id(image),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;

    #[test]
    fn catalogue_filter_values_preserve_inheritance_and_empty_overrides() {
        let sources = [
            (
                "building.xml",
                r#"<BFTypedBinder binderType="building" abstract="true"><types><entity><building/></entity></types><shared><BFAIEntityDataShared s_ObjectType="Buildings" s_Theme="Jungle" b_Researchable="false"/></shared></BFTypedBinder>"#,
            ),
            (
                "cart.xml",
                r#"<BFTypedBinder binderType="cart"><types><entity><building><cart/></building></entity></types><shared><BFAIEntityDataShared s_ObjectType="Carts" s_Theme=""/></shared></BFTypedBinder>"#,
            ),
        ];
        let documents = sources.map(|(path, source)| {
            parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
                .expect("valid binder")
        });
        let index = SourceIndex::build(&documents).expect("valid inheritance");
        let values = authored_catalogue_filter_values(&index.find("cart").expect("cart binder"));
        assert!(values
            .iter()
            .any(|value| value.field == "s_objecttype" && value.value == "Carts"));
        assert!(values
            .iter()
            .any(|value| value.field == "b_researchable" && value.value == "false"));
        assert!(!values.iter().any(|value| value.field == "s_theme"));
        assert!(!values.iter().any(|value| value.value == "Buildings"));
    }
}

pub(super) fn bind_authored_placement_catalogue_entry(
    record: &RecordView<'_, '_>,
    object_kind: WorldObjectKind,
    name_key: AssetId,
    icon: AssetId,
    output: &mut WorldDefinitionLoweringTables,
) {
    let mut type_tokens = record.type_tokens();
    if type_tokens.is_empty() {
        type_tokens.push(record.key.to_owned());
    }
    let catalogue_kind = type_tokens
        .last()
        .map(|value| id(value))
        .unwrap_or_else(|| id(record.key));
    let kinds = type_tokens.iter().map(|value| id(value)).collect();
    let category = if record.has_type_token("animal") {
        CatalogueCategory::Animals
    } else if record.has_type_token("staff") {
        CatalogueCategory::Staff
    } else if record.has_type_token("fence") || record.has_type_token("gate") {
        CatalogueCategory::Fences
    } else if record.has_type_token("path") || record.has_type_token("curb") {
        CatalogueCategory::Paths
    } else if record.has_type_token("tank") {
        CatalogueCategory::Tanks
    } else if record.has_type_token("showstage") {
        CatalogueCategory::Shows
    } else if record.has_type_token("station")
        || record.has_type_token("track")
        || record.has_type_token("vehicle")
    {
        CatalogueCategory::Transport
    } else if matches!(
        object_kind,
        WorldObjectKind::Facility
            | WorldObjectKind::Shelter
            | WorldObjectKind::Food
            | WorldObjectKind::Water
            | WorldObjectKind::DonationBox
            | WorldObjectKind::Bin
            | WorldObjectKind::Bench
            | WorldObjectKind::Laboratory
    ) {
        CatalogueCategory::Facilities
    } else {
        CatalogueCategory::Scenery
    };
    let catalogue_filters = if record.has_type_token("animal") {
        CatalogueFilterFlags::ADOPTABLE
    } else if record.has_type_token("staff") {
        CatalogueFilterFlags::HIREABLE
    } else {
        CatalogueFilterFlags::PURCHASABLE.with_additional_flags(CatalogueFilterFlags::BUILDABLE)
    };
    let authored_purchase_sort_key = authored_catalogue_purchase_sort_key(record);
    output.document.catalogue.push(CatalogueEntry {
        filter_values: authored_catalogue_filter_values(record),
        id: id(&format!("catalogue/{}", record.key)),
        definition: id(record.key),
        kind: catalogue_kind,
        kinds,
        category,
        authored_purchase_sort_key,
        authored_purchase_sort_fallback_type_name:
            authored_catalogue_purchase_sort_fallback_type_name(record),
        authored_type_registry_source_order: [u64::MAX; 3],
        filters: catalogue_filters,
        name_key,
        icon,
    });
}
