use openzt2_game_data::{
    AssetId,
    world_definitions::biomes_locations_and_details::{
        BiomeAutomaticPlacementChoice, BiomeAutomaticPlacementRange,
    },
};

use super::{
    ordered_source_document_types::OrderedSourceDocumentNode,
    source_document_semantic_name::canonicalize_source_document_record_key,
    ui::model::SourceUiPayloadNode,
};

pub(crate) trait AutomaticPlacementElement {
    fn name(&self) -> &str;
    fn attribute(&self, name: &str) -> Option<&str>;
    fn children(&self) -> impl Iterator<Item = &Self>;
}

impl AutomaticPlacementElement for OrderedSourceDocumentNode {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attribute_named_any(&[name])
    }

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.element_children()
    }
}

impl AutomaticPlacementElement for SourceUiPayloadNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find_map(|attribute| {
            attribute
                .name
                .eq_ignore_ascii_case(name)
                .then_some(attribute.value.as_str())
        })
    }

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.children.iter()
    }
}

pub(crate) fn automatic_placement_range_index(name: &str) -> Result<usize, String> {
    name.to_ascii_lowercase()
        .strip_prefix("range")
        .and_then(|index| index.parse().ok())
        .ok_or_else(|| format!("invalid automatic-placement range {name:?}"))
}

pub(crate) fn lower_automatic_placement_choices(
    range: &impl AutomaticPlacementElement,
) -> Result<Vec<BiomeAutomaticPlacementChoice>, String> {
    range
        .children()
        .map(|choice| {
            let weight = choice
                .attribute("weight")
                .and_then(|weight| weight.parse::<u32>().ok())
                .filter(|&weight| weight != 0)
                .ok_or_else(|| {
                    format!("invalid automatic-placement weight for {:?}", choice.name())
                })?;
            let key = canonicalize_source_document_record_key(choice.name());
            Ok(BiomeAutomaticPlacementChoice {
                object: (key != "nothing").then(|| AssetId::from_key(&key)),
                weight,
            })
        })
        .collect()
}

pub(crate) fn lower_automatic_placement_ranges(
    table: &impl AutomaticPlacementElement,
) -> Result<Vec<BiomeAutomaticPlacementRange>, String> {
    let mut indexed = table
        .children()
        .map(|range| {
            let index = automatic_placement_range_index(range.name())?;
            let maximum_mask_value = range
                .attribute("threshold")
                .and_then(|value| value.parse::<u8>().ok())
                .ok_or_else(|| {
                    format!(
                        "invalid automatic-placement threshold for {:?}",
                        range.name()
                    )
                })?;
            Ok((
                index,
                BiomeAutomaticPlacementRange {
                    maximum_mask_value,
                    choices: lower_automatic_placement_choices(range)?,
                },
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    indexed.sort_unstable_by_key(|(index, _)| *index);
    if indexed
        .iter()
        .enumerate()
        .any(|(index, (authored, _))| index != *authored)
        || indexed
            .windows(2)
            .any(|pair| pair[0].1.maximum_mask_value >= pair[1].1.maximum_mask_value)
    {
        return Err(
            "automatic-placement ranges require contiguous indexes and ascending thresholds"
                .to_owned(),
        );
    }
    Ok(indexed.into_iter().map(|(_, range)| range).collect())
}
