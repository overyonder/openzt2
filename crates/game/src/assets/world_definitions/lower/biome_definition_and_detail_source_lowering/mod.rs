use super::source_element_tree_search::find_descendant;
use super::source_model_scene_path_normalization::normalize_scene_path;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, asset_list, element_array, id,
};
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentNode,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::biomes_locations_and_details::{
    BiomeAutomaticPlacementChoice, BiomeAutomaticPlacementRange, BiomeAutomaticPlacementVariation,
    BiomeAutomaticPlacementVariationKind, BiomeDefinition, BiomeDetailChoice, BiomeDetailLevel,
    BiomeDetailPlacementPolicy, BiomeDetailPolicy, BiomeDetailSurface, BiomeOverviewMapColors,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_biome_detail_placement(
    documents: &[OrderedSourceDocument],
) -> Result<Option<BiomeDetailPlacementPolicy>, BindError> {
    let mut policies = documents.iter().filter_map(|document| {
        document
            .root
            .name
            .eq_ignore_ascii_case("BFTerrain")
            .then(|| {
                document
                    .root
                    .element_children()
                    .find(|node| node.name.eq_ignore_ascii_case("DetailObjects"))
                    .map(|node| (document, node))
            })
            .flatten()
    });
    let Some((document, node)) = policies.next() else {
        return Ok(None);
    };
    if policies.next().is_some() {
        return Err(BindError {
            virtual_path: document.path.key(),
            span: node.span,
            message: "multiple resolved BFTerrain detail placement policies".to_owned(),
        });
    }
    let parse_metres = |name: &str| -> Result<u16, BindError> {
        let value = node.attribute(name).ok_or_else(|| BindError {
            virtual_path: document.path.key(),
            span: node.span,
            message: format!("BFTerrain DetailObjects has no {name}"),
        })?;
        let metres = value
            .trim()
            .trim_end_matches(|character| character == 'f' || character == 'F')
            .parse::<f64>()
            .map_err(|_| BindError {
                virtual_path: document.path.key(),
                span: node.span,
                message: format!("BFTerrain DetailObjects {name} is not numeric"),
            })?;
        let cm = (metres * 100.0).round();
        if !metres.is_finite() || cm <= 0.0 || cm > f64::from(u16::MAX) {
            return Err(BindError {
                virtual_path: document.path.key(),
                span: node.span,
                message: format!("BFTerrain DetailObjects {name} is outside the supported range"),
            });
        }
        Ok(cm as u16)
    };
    Ok(Some(BiomeDetailPlacementPolicy {
        region_size_cm: parse_metres("regionSize")?,
        radius_cm: parse_metres("radius")?,
        solid_distance_cm: parse_metres("solidDistance")?,
        fade_distance_cm: parse_metres("fadeDistance")?,
    }))
}

pub(super) fn bind_biome(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let foliage = record
        .value(&["foliage", "objects"])
        .map(asset_list)
        .unwrap_or_default();
    let foliage = foliage;
    let biome = id(record.key);
    let detail_policies = bind_biome_detail_objects(record)?;
    let overview_colors = record
        .descendant_named("overviewColors")
        .map(|colors| {
            Ok(BiomeOverviewMapColors {
                covered_ground: element_array(
                    &colors,
                    &["coverR", "coverG", "coverB", "coverA"],
                    [0, 0, 0, 255],
                )?,
                uncovered_ground: element_array(
                    &colors,
                    &["uncoverR", "uncoverG", "uncoverB", "uncoverA"],
                    [0, 0, 0, 255],
                )?,
                cliff: element_array(
                    &colors,
                    &["cliffR", "cliffG", "cliffB", "cliffA"],
                    [0, 0, 0, 255],
                )?,
            })
        })
        .transpose()?
        .unwrap_or(BiomeOverviewMapColors {
            covered_ground: [0, 0, 0, 255],
            uncovered_ground: [0, 0, 0, 255],
            cliff: [0, 0, 0, 255],
        });
    output.document.biomes.push(BiomeDefinition {
        id: biome,
        name_key: asset(record, &["nameKey", "displayName", "displayNameToken"]),
        icon: asset(record, &["iconName", "icon"]),
        terrain_material: asset(
            record,
            &["terrainMaterial", "groundTexture", "groundMaterial"],
        ),
        cliff_horizontal: asset(record, &["cliffHoriz"]),
        cliff_vertical: asset(record, &["cliffVert"]),
        cliff_diagonal_ascending: asset(record, &["cliffDiag1"]),
        cliff_diagonal_descending: asset(record, &["cliffDiag2"]),
        cliff_unaligned: asset(record, &["cliffUnaligned"]),
        water_material: asset(record, &["waterMaterial", "waterTexture"]),
        automatic_placement_mask: asset(record, &["objectMask"]),
        automatic_placement_variations: bind_biome_automatic_placement_variations(record)?,
        foliage,
        detail_policies,
        overview_colors,
        temperature_c: array(
            record,
            &["minimumTemperatureC", "maximumTemperatureC"],
            [0_i16; 2],
        )?,
        humidity_permille: array(
            record,
            &["minimumHumidityPermille", "maximumHumidityPermille"],
            [0_u16; 2],
        )?,
    });
    Ok(())
}

fn bind_biome_automatic_placement_variations(
    record: &RecordView<'_, '_>,
) -> Result<Vec<BiomeAutomaticPlacementVariation>, BindError> {
    let authored_variations = record
        .descendant_named("autoPlacementVariations")
        .map(|variations| {
            variations
                .element_children()
                .map(|variation| (variation.name.as_str(), variation))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            ["foliage-mix", "foliage-mix-terrain"]
                .into_iter()
                .filter_map(|name| {
                    record
                        .descendant_with_attribute("UIToggleButton", "name", name)
                        .and_then(|button| find_descendant(button, "autoPlacement"))
                        .map(|auto_placement| (name, auto_placement))
                })
                .collect::<Vec<_>>()
        });
    authored_variations
        .into_iter()
        .map(|(variation_name, ranges)| {
            let kind = biome_automatic_placement_variation_kind(record, variation_name)?;
            let mut indexed_ranges = ranges
                .element_children()
                .map(|range| {
                    let index = biome_automatic_placement_range_index(record, range.name.as_str())?;
                    let maximum_mask_value = range
                        .attribute_named_any(&["threshold"])
                        .ok_or_else(|| {
                            BindError::record(
                                record,
                                format!(
                                    "biome automatic-placement {} has no threshold",
                                    range.name.as_str()
                                ),
                            )
                        })?
                        .parse::<u8>()
                        .map_err(|_| {
                            BindError::record(
                                record,
                                format!(
                                    "biome automatic-placement {} threshold is not a byte",
                                    range.name.as_str()
                                ),
                            )
                        })?;
                    Ok((
                        index,
                        BiomeAutomaticPlacementRange {
                            maximum_mask_value,
                            choices: bind_biome_automatic_placement_choices(record, range)?,
                        },
                    ))
                })
                .collect::<Result<Vec<_>, BindError>>()?;
            indexed_ranges.sort_unstable_by_key(|(index, _)| *index);
            if indexed_ranges
                .iter()
                .enumerate()
                .any(|(expected_index, (authored_index, _))| {
                    usize::from(*authored_index) != expected_index
                })
            {
                return Err(BindError::record(
                    record,
                    format!(
                        "biome automatic-placement {} ranges are not contiguous from Range0",
                        variation_name
                    ),
                ));
            }
            for window in indexed_ranges.windows(2) {
                if window[0].1.maximum_mask_value >= window[1].1.maximum_mask_value {
                    return Err(BindError::record(
                        record,
                        format!(
                            "biome automatic-placement {} ranges are not uniquely ordered",
                            variation_name
                        ),
                    ));
                }
            }
            Ok(BiomeAutomaticPlacementVariation {
                kind,
                ranges: indexed_ranges.into_iter().map(|(_, range)| range).collect(),
            })
        })
        .collect()
}

fn bind_biome_automatic_placement_choices(
    record: &RecordView<'_, '_>,
    range: &'_ OrderedSourceDocumentNode,
) -> Result<Vec<BiomeAutomaticPlacementChoice>, BindError> {
    range
        .element_children()
        .map(|choice| {
            let weight = choice
                .attribute_named_any(&["weight"])
                .ok_or_else(|| {
                    BindError::record(record, "biome automatic-placement choice has no weight")
                })?
                .parse::<u32>()
                .map_err(|_| {
                    BindError::record(
                        record,
                        "biome automatic-placement choice weight is not a positive u32",
                    )
                })?;
            if weight == 0 {
                return Err(BindError::record(
                    record,
                    "biome automatic-placement choice weight is zero",
                ));
            }
            Ok(BiomeAutomaticPlacementChoice {
                object: (!source_document_names_are_semantically_equal(
                    choice.name.as_str(),
                    "nothing",
                ))
                .then(|| id(choice.name.as_str())),
                weight,
            })
        })
        .collect()
}

fn biome_automatic_placement_variation_kind(
    record: &RecordView<'_, '_>,
    name: &str,
) -> Result<BiomeAutomaticPlacementVariationKind, BindError> {
    match name.to_ascii_lowercase().as_str() {
        "foliage-mix" => Ok(BiomeAutomaticPlacementVariationKind::FoliageMix),
        "foliage-mix-terrain" => Ok(BiomeAutomaticPlacementVariationKind::FoliageMixTerrain),
        "water" => Ok(BiomeAutomaticPlacementVariationKind::Water),
        _ => Err(BindError::record(
            record,
            format!("unsupported biome automatic-placement variation {name}"),
        )),
    }
}

fn biome_automatic_placement_range_index(
    record: &RecordView<'_, '_>,
    name: &str,
) -> Result<u16, BindError> {
    name.strip_prefix("Range")
        .or_else(|| name.strip_prefix("range"))
        .and_then(|index| index.parse::<u16>().ok())
        .ok_or_else(|| {
            BindError::record(
                record,
                format!("invalid biome automatic-placement range name {name}"),
            )
        })
}

pub(super) fn apply_biome_automatic_placement_supplements(
    documents: &[OrderedSourceDocument],
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    for document in documents
        .iter()
        .filter(|document| document.root.name.eq_ignore_ascii_case("BFGBiomeObjects"))
    {
        let path = document.path.key();
        let mut components = path.split('/');
        let biome_name = match (components.next(), components.next(), components.next()) {
            (Some("biomes"), Some("autoplacement"), Some(biome_name)) => biome_name,
            _ => {
                return Err(BindError {
                    virtual_path: path,
                    span: document.root.span,
                    message: "BFGBiomeObjects is outside biomes/autoplacement/<biome>".to_owned(),
                });
            }
        };
        let biome_id = id(biome_name);
        let biome = output
            .document
            .biomes
            .iter_mut()
            .find(|biome| biome.id == biome_id)
            .ok_or_else(|| BindError {
                virtual_path: path.clone(),
                span: document.root.span,
                message: format!("BFGBiomeObjects has no resolved base biome {biome_name}"),
            })?;
        for variation_node in document.root.element_children() {
            let kind = biome_automatic_placement_variation_kind_for_document(
                document,
                variation_node.name.as_str(),
            )?;
            let variation = biome
                .automatic_placement_variations
                .iter_mut()
                .find(|variation| variation.kind == kind)
                .ok_or_else(|| BindError {
                    virtual_path: path.clone(),
                    span: variation_node.span,
                    message: format!(
                        "BFGBiomeObjects variation {} has no base variation",
                        variation_node.name
                    ),
                })?;
            let auto_placement = variation_node
                .element_children()
                .find(|child| child.name.eq_ignore_ascii_case("autoPlacement"))
                .ok_or_else(|| BindError {
                    virtual_path: path.clone(),
                    span: variation_node.span,
                    message: format!(
                        "BFGBiomeObjects variation {} has no autoPlacement",
                        variation_node.name
                    ),
                })?;
            for range_node in auto_placement.element_children() {
                let index = biome_automatic_placement_range_index_for_document(
                    document,
                    range_node.name.as_str(),
                )?;
                let Some(range) = variation.ranges.get_mut(usize::from(index)) else {
                    return Err(BindError {
                        virtual_path: path.clone(),
                        span: range_node.span,
                        message: format!(
                            "BFGBiomeObjects range {} has no base range",
                            range_node.name
                        ),
                    });
                };
                if let Some(threshold) = range_node
                    .attributes
                    .iter()
                    .find(|attribute| attribute.name.as_str().eq_ignore_ascii_case("threshold"))
                {
                    let supplemental_threshold =
                        threshold.value().parse::<u8>().map_err(|_| BindError {
                            virtual_path: path.clone(),
                            span: range_node.span,
                            message: format!(
                                "BFGBiomeObjects range {} threshold is not a byte",
                                range_node.name
                            ),
                        })?;
                    if supplemental_threshold != range.maximum_mask_value {
                        return Err(BindError {
                            virtual_path: path.clone(),
                            span: range_node.span,
                            message: format!(
                                "BFGBiomeObjects range {} threshold differs from its base range",
                                range_node.name
                            ),
                        });
                    }
                }
                for choice in range_node.element_children() {
                    let weight = choice
                        .attributes
                        .iter()
                        .find(|attribute| attribute.name.as_str().eq_ignore_ascii_case("weight"))
                        .ok_or_else(|| BindError {
                            virtual_path: path.clone(),
                            span: choice.span,
                            message: "BFGBiomeObjects choice has no weight".to_owned(),
                        })?
                        .value()
                        .parse::<u32>()
                        .map_err(|_| BindError {
                            virtual_path: path.clone(),
                            span: choice.span,
                            message: "BFGBiomeObjects choice weight is not a positive u32"
                                .to_owned(),
                        })?;
                    if weight == 0 {
                        return Err(BindError {
                            virtual_path: path.clone(),
                            span: choice.span,
                            message: "BFGBiomeObjects choice weight is zero".to_owned(),
                        });
                    }
                    range.choices.push(BiomeAutomaticPlacementChoice {
                        object: (!choice.name.eq_ignore_ascii_case("nothing"))
                            .then(|| id(choice.name.as_str())),
                        weight,
                    });
                }
            }
        }
    }
    Ok(())
}

fn biome_automatic_placement_variation_kind_for_document(
    document: &OrderedSourceDocument,
    name: &str,
) -> Result<BiomeAutomaticPlacementVariationKind, BindError> {
    match name.to_ascii_lowercase().as_str() {
        "foliage-mix" => Ok(BiomeAutomaticPlacementVariationKind::FoliageMix),
        "foliage-mix-terrain" => Ok(BiomeAutomaticPlacementVariationKind::FoliageMixTerrain),
        "water" => Ok(BiomeAutomaticPlacementVariationKind::Water),
        _ => Err(BindError {
            virtual_path: document.path.key(),
            span: document.root.span,
            message: format!("unsupported BFGBiomeObjects variation {name}"),
        }),
    }
}

fn biome_automatic_placement_range_index_for_document(
    document: &OrderedSourceDocument,
    name: &str,
) -> Result<u16, BindError> {
    name.strip_prefix("Range")
        .or_else(|| name.strip_prefix("range"))
        .and_then(|index| index.parse::<u16>().ok())
        .ok_or_else(|| BindError {
            virtual_path: document.path.key(),
            span: document.root.span,
            message: format!("invalid BFGBiomeObjects range name {name}"),
        })
}

pub(super) fn bind_biome_detail_objects(
    record: &RecordView<'_, '_>,
) -> Result<Vec<BiomeDetailPolicy>, BindError> {
    let mut policies = Vec::new();
    if let Some(root) = record.descendant_named("detailobjects") {
        for level_node in root.element_children() {
            let level =
                match canonicalize_source_document_record_key(level_node.name.as_str()).as_str() {
                    "lowest" => BiomeDetailLevel::Lowest,
                    "low" => BiomeDetailLevel::Low,
                    "med" | "medium" => BiomeDetailLevel::Medium,
                    "high" => BiomeDetailLevel::High,
                    other => {
                        return Err(BindError::record(
                            record,
                            format!("unsupported biome detail level {other}"),
                        ));
                    }
                };
            bind_biome_detail_level(record, &mut policies, level, level_node)?;
        }
    }
    if let Some(root) = record.descendant_named("decorativedetailobjects") {
        bind_biome_detail_level(record, &mut policies, BiomeDetailLevel::Decorative, root)?;
    }
    Ok(policies)
}

pub(super) fn bind_biome_detail_level(
    record: &RecordView<'_, '_>,
    policies: &mut Vec<BiomeDetailPolicy>,
    level: BiomeDetailLevel,
    node: &'_ OrderedSourceDocumentNode,
) -> Result<(), BindError> {
    for surface in node.element_children() {
        let surface_kind =
            match canonicalize_source_document_record_key(surface.name.as_str()).as_str() {
                "dirt" | "ground" => BiomeDetailSurface::Ground,
                "cover" => BiomeDetailSurface::Cover,
                "shore" => BiomeDetailSurface::Shore,
                other => {
                    return Err(BindError::record(
                        record,
                        format!("unsupported biome detail surface {other}"),
                    ));
                }
            };
        let density = surface
            .attribute_named_any(&["objDensity", "density"])
            .unwrap_or("1")
            .parse::<u16>()
            .map_err(|_| BindError::record(record, "biome detail density is not a positive u16"))?;
        if density == 0 {
            return Err(BindError::record(record, "biome detail density is zero"));
        }
        let mut choices = Vec::new();
        for choice in surface.element_children().filter(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "object")
        }) {
            let weight = choice
                .attribute_named_any(&["probability", "weight"])
                .ok_or_else(|| BindError::record(record, "biome detail choice has no probability"))?
                .parse::<u32>()
                .map_err(|_| {
                    BindError::record(record, "biome detail probability is not a positive u32")
                })?;
            if weight == 0 {
                return Err(BindError::record(
                    record,
                    "biome detail probability is zero",
                ));
            }
            let prefab = choice
                .attribute_named_any(&["nifFile", "model"])
                .unwrap_or("")
                .trim();
            choices.push(BiomeDetailChoice {
                prefab: (!prefab.is_empty())
                    .then(|| AssetId::from_virtual_path(&normalize_scene_path(prefab)))
                    .unwrap_or_default(),
                weight,
            });
        }
        if !choices.is_empty() {
            policies.push(BiomeDetailPolicy {
                level,
                surface: surface_kind,
                density,
                choices,
            });
        }
    }
    Ok(())
}
