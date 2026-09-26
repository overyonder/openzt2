//! View rows use the existing derived-first source inheritance walk.

use super::source_element_tree_search::authored_type_family_components;
use super::world_definition_source_value_reading_and_conversion::{
    id, required_element_number, simple_error,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::world_objects::WorldObjectViewData;
use openzt2_game_data::AssetId;
use std::collections::BTreeMap;

pub(super) fn lower_view_data(
    record: &RecordView<'_, '_>,
) -> Result<Vec<WorldObjectViewData>, BindError> {
    // The source binder merges attributes on named rows: a child overriding only
    // viewScore still inherits the other scores. Apply native zero defaults only
    // after visiting the entire family, not when first encountering a row.
    let mut rows: BTreeMap<AssetId, [Option<f32>; 4]> = BTreeMap::new();
    for component in authored_type_family_components(record, "ZTAIViewComponent") {
        for table in component
            .element_children()
            .filter(|node| node.name.as_str() == "viewDataTable")
        {
            for row in table
                .element_children()
                .filter(|node| node.name.as_str() == "ZTAIViewData")
            {
                let name = id(row
                    .attribute_named_any(&["name"])
                    .ok_or_else(|| simple_error("ZTAIViewData has no name"))?);
                let values = rows.entry(name).or_default();
                for (value, key) in
                    values
                        .iter_mut()
                        .zip(["viewScore", "tourScore", "donateScore", "entScore"])
                {
                    if value.is_none() && row.attribute_named_any(&[key]).is_some() {
                        let parsed = required_element_number::<f32>(&row, &[key])?;
                        if !parsed.is_finite() {
                            return Err(simple_error("ZTAIViewData has nonfinite score"));
                        }
                        *value = Some(parsed);
                    }
                }
            }
        }
    }
    Ok(rows
        .into_iter()
        .map(|(name, values)| {
            let [view_score, tour_score, donation_score, entertainment_score] =
                values.map(|value| value.unwrap_or(0.0));
            WorldObjectViewData {
                name,
                view_score,
                tour_score,
                donation_score,
                entertainment_score,
            }
        })
        .collect())
}
