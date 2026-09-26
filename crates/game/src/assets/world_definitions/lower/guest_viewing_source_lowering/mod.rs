use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    id, parse, parse_f32_triplet, required_number,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestViewingPolicy, ViewingOpportunityDefinition, ViewingOpportunitySlot,
};

#[derive(Clone)]
pub(super) struct ViewingTemplate {
    pub(super) priority: i16,
    pub(super) slots: Vec<[f32; 3]>,
}

pub(super) fn bind_viewing_template(
    records: &[RecordView<'_, '_>],
) -> Result<Option<ViewingTemplate>, BindError> {
    let Some(record) = records
        .iter()
        .find(|record| record.key.eq_ignore_ascii_case("viewingarea"))
    else {
        return Ok(None);
    };
    let Some(shared) = record.descendant_named("BFAIEntityDataShared") else {
        return Err(BindError::record(
            record,
            "ViewingArea is missing BFAIEntityDataShared",
        ));
    };
    let priority = shared
        .attribute_named_any(&["f_ViewPriority"])
        .map(|value| parse(value, record, "f_ViewPriority"))
        .transpose()?
        .unwrap_or(1_i16);
    let mut slots = Vec::new();
    for index in 1..=32 {
        let name = format!("p_Dock_{index:02}");
        let Some(value) = shared.attribute_named_any(&[name.as_str()]) else {
            continue;
        };
        slots.push(parse_f32_triplet(value).map_err(|_| {
            BindError::record(
                record,
                format!("invalid authored viewing slot {name}: {value}"),
            )
        })?);
    }
    if slots.is_empty() {
        return Err(BindError::record(
            record,
            "ViewingArea has no authored p_Dock slots",
        ));
    }
    Ok(Some(ViewingTemplate { priority, slots }))
}

pub(super) fn bind_viewing_opportunity(
    record: &RecordView<'_, '_>,
    template: Option<&ViewingTemplate>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if !record.has_type_token("ViewingArea")
        || record.value(&["abstract"]).is_some_and(|value| {
            matches!(
                canonicalize_source_document_record_key(value).as_str(),
                "true" | "yes" | "1"
            )
        })
    {
        return Ok(());
    }
    let template = template.ok_or_else(|| {
        BindError::record(
            record,
            "concrete ViewingArea has no resolved authored slot template",
        )
    })?;
    let slots = template
        .slots
        .iter()
        .copied()
        .map(|local_position_m| ViewingOpportunitySlot { local_position_m })
        .collect();
    output
        .document
        .viewing_opportunities
        .push(ViewingOpportunityDefinition {
            id: id(record.key),
            priority: template.priority,
            slots,
        });
    Ok(())
}

pub(super) fn bind_guest_viewing_policy(
    record: &RecordView<'_, '_>,
) -> Result<GuestViewingPolicy, BindError> {
    fn descendant_attribute<'a>(
        element: &'a OrderedSourceDocumentNode,
        name: &str,
    ) -> Option<&'a str> {
        element.attribute_named_any(&[name]).or_else(|| {
            element
                .element_children()
                .find_map(|child| descendant_attribute(child, name))
        })
    }
    let maximum_slope_cos = descendant_attribute(record.source_document_element(), "maxSlope")
        .map(|value| parse(value, record, "maxSlope"))
        .transpose()?
        .unwrap_or(0.75);
    Ok(GuestViewingPolicy {
        ground_distance_m: required_number(record, &["viewingDistanceGround"])?,
        ground_range_m: required_number(record, &["viewingRangeGround"])?,
        guest_eye_height_m: required_number(record, &["guestViewHeight"])?,
        minimum_open_cells: required_number(record, &["minOpenAreaSize"])?,
        maximum_slope_cos,
    })
}
