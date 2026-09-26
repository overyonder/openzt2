use crate::assets::source_document::resolved_source_record_index::{
    BindError,
    RecordView,
};
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::{
    AnimalAdoptionFameSlotThreshold,
    AnimalAdoptionOfferConfiguration,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::id;

pub(super) fn bind_authored_animal_adoption_offer_configuration(
    record: &RecordView<'_, '_>,
    resolved_source_records: &[RecordView<'_, '_>],
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !source_document_names_are_semantically_equal(
        record.source_document_element().name.as_str(),
        "ZTAdoptionSlotMgr",
    ) {
        return Ok(());
    }
    let base_slot_count = record
        .source_document_element()
        .attribute_named_any(&["slots"])
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| BindError::record(record, "ZTAdoptionSlotMgr has no valid slots"))?;
    let fame_slots = record
        .source_document_element()
        .element_children()
        .find(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "fameslots")
        })
        .into_iter()
        .flat_map(|container| container.element_children())
        .filter(|child| source_document_names_are_semantically_equal(child.name.as_str(), "entry"))
        .map(|entry| {
            let fame_percent = entry
                .attribute_named_any(&["fame"])
                .and_then(|value| value.parse::<u16>().ok())
                .ok_or_else(|| BindError::record(record, "adoption fame slot has invalid fame"))?;
            let additional_slot_count = entry
                .attribute_named_any(&["slots"])
                .and_then(|value| value.parse::<u16>().ok())
                .ok_or_else(|| BindError::record(record, "adoption fame slot has invalid slots"))?;
            Ok(AnimalAdoptionFameSlotThreshold {
                fame_percent,
                additional_slot_count,
                locked_slot_long_tooltip_key: entry
                    .attribute_named_any(&["locid"])
                    .map(id)
                    .unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    let installed_expansion_slot_count = resolved_source_records
        .iter()
        .filter(|candidate| {
            source_document_names_are_semantically_equal(
                candidate.source_document_element().name.as_str(),
                "ZTExpansionInfo",
            ) && candidate
                .source_document_element()
                .attribute_named_any(&["installed"])
                .is_none_or(|installed| {
                    matches!(installed.trim().to_ascii_lowercase().as_str(), "true" | "1")
                })
        })
        .filter_map(|candidate| {
            candidate
                .source_document_element()
                .attribute_named_any(&["extraAdoptionSlots"])
                .and_then(|value| value.parse::<u16>().ok())
        })
        .fold(0_u16, u16::saturating_add);
    output.document.animal_adoption_offer_configuration = Some(AnimalAdoptionOfferConfiguration {
        base_slot_count,
        installed_expansion_slot_count,
        fame_slots,
    });
    Ok(())
}
