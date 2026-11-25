use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::source_document::ui::model::{SourceUiEvent, SourceUiPayloadNode};
use crate::ui_document::source::lower::authored_ui_action_argument_lowering::{
    event_i32, required_bool, required_i8, required_tenths,
};
use crate::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use crate::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::construction::{
    UiBiomePaintClass, UiConstructionAction, UiConstructionActionRecord, UiFencePlacementMode,
    UiTankEditMode, UiTerrainEditMode,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::world_definitions::biomes_locations_and_details::{
    BiomeAutomaticPlacementChoice, BiomeAutomaticPlacementRange,
};
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_construction_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_EXITMODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SelectInspectionTool,
        })),
        "ZT_OBJECT_ROTATE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::RotateSelected {
                steps: match event.string.as_deref() {
                    None | Some("right") => 1,
                    Some("left") => -1,
                    Some(value) => {
                        return Err(invalid_at(input, format!("unknown object-rotation direction {value:?}")));
                    }
                },
            },
        })),
        "ZT_TERRAINCURSOR_SIZE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetTerrainCursorSize {
                direction: match event.string.as_deref().or(event.value.as_deref()) {
                    Some("+") => 1,
                    Some("-") => -1,
                    _ => event_i32(event, 0),
                },
            },
        })),
        "ZT_SETPLACEMENTOBJECT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetPlacementObject {
                definition: canonical_source_value_resolution::lower_optional_authored_object_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
            },
        })),
        "ZT_UNDOACTION" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::Undo,
        })),
        "ZT_SETSINGLEOBJECTPLACEMENT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetSingleObjectPlacement,
        })),
        "ZT_CRATE_ENTITY" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::CrateSelected,
        })),
        "ZT_SELL_ENTITY" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SellSelected,
        })),
        "ZT_MOVE_SELECTION_ENTITY" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::MoveSelected,
        })),
        "ZT_SELL_PHYSOBJ" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SellPhysicalObject {
                definition: canonical_source_value_resolution::lower_optional_authored_object_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
            },
        })),
        "ZT_CLEARMODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SelectInspectionTool,
        })),
        "ZT_SETFENCEPLACEMENTMODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetFencePlacementMode {
                mode: match event.string.as_deref().or(event.value.as_deref()).unwrap_or_default() {
                    "rubber-band" => UiFencePlacementMode::RubberBand,
                    "rectangle" => UiFencePlacementMode::Rectangle,
                    "octagon" => UiFencePlacementMode::Octagon,
                    value => {
                        return Err(invalid_at(input, format!("unsupported fence placement mode {value:?}")));
                    }
                },
            },
        })),
        "ZT_SETTANKMODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetTankEditMode {
                mode: match event.string.as_deref().or(event.value.as_deref()).unwrap_or_default() {
                    "raiseWall" => UiTankEditMode::RaiseWall,
                    "lowerWall" => UiTankEditMode::LowerWall,
                    "raiseFloor" => UiTankEditMode::RaiseFloor,
                    "lowerFloor" => UiTankEditMode::LowerFloor,
                    "fill" => UiTankEditMode::Fill,
                    "drain" => UiTankEditMode::Drain,
                    value => {
                        return Err(invalid_at(input, format!("unsupported tank edit mode {value:?}")));
                    }
                },
            },
        })),
        "ZT_SETTERRAINMODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetTerrainEditMode {
                mode: match event.string.as_deref().or(event.value.as_deref()).unwrap_or_default() {
                    "hill" => UiTerrainEditMode::Hill,
                    "valley" => UiTerrainEditMode::Valley,
                    "flatten" => UiTerrainEditMode::Flatten,
                    "smooth" => UiTerrainEditMode::Smooth,
                    value => {
                        return Err(invalid_at(input, format!("unsupported terrain edit mode {value:?}")));
                    }
                },
            },
        })),
        "ZT_SETTERRAINFLATTEN_HEIGHT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetTerrainFlattenHeight {
                height_decimetres: required_tenths(event, input)?,
            },
        })),
        "ZT_SETTERRAINFLATTEN_SPEED" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetTerrainFlattenSpeed {
                strength_tenths_per_second: required_tenths(event, input)?,
            },
        })),
        "ZT_ELEVATED_PATH_PLACEMENT_MODE" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetElevatedPathPlacementMode {
                grade_steps: required_i8(event, input)?,
            },
        })),
        "ZT_ELEVATED_PATH_PLACEMENT_HEIGHT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::AdjustElevatedPathHeight { steps: required_i8(event, input)? },
        })),
        "ZT_SKYTOWER_PLACEMENT_HEIGHT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::AdjustSkyTowerHeight { steps: required_i8(event, input)? },
        })),
        "ZT_BIOME_PAINT_FOLIAGE" | "ZT_BIOME_PAINT_ROCKS" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetBiomePaintClass {
                detail_class: if event.message == "ZT_BIOME_PAINT_FOLIAGE" {
                    UiBiomePaintClass::Foliage
                } else {
                    UiBiomePaintClass::Rocks
                },
                enabled: event.value.as_deref().and_then(parse_bool).ok_or_else(|| invalid_at(input, "biome paint toggle requires a bool"))?,
            },
        })),
        "ZT_EYEDROPPER" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::Eyedropper,
        })),
        "ZT_UNCRATE_ENTITY" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::UncrateSelected,
        })),
        "ZT_BIOME_PAINT_TREES" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetBiomePaintTrees {
                enabled: event.value.as_deref().and_then(parse_bool).unwrap_or(false),
            },
        })),
        "ZT_SET_AUTO_PLACEMENT_LIST" => {
            let [table] = event.payload.as_slice() else {
                return Err(invalid_at(input, "ZT_SET_AUTO_PLACEMENT_LIST requires an autoPlacement table"));
            };
            let ranges = lower_automatic_placement_ranges(table).map_err(|message| invalid_at(input, message))?;
            Ok(UiActionRecord::Construction(UiConstructionActionRecord {
                trigger,
                action: UiConstructionAction::SetBiomeAutomaticPlacementTable { ranges },
            }))
        }
        "ZT_SET_BIOME" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetBiome {
                biome: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
            },
        })),
        "ZT_CONFIRM_ENTITY_PLACEMENT" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::ConfirmPlacement {
                confirmed: required_bool(event, input)?,
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}

pub(crate) fn lower_automatic_placement_ranges(
    table: &SourceUiPayloadNode,
) -> Result<Vec<BiomeAutomaticPlacementRange>, String> {
    fn attribute<'a>(node: &'a SourceUiPayloadNode, name: &str) -> Option<&'a str> {
        node.attributes.iter().find_map(|value| {
            value
                .name
                .eq_ignore_ascii_case(name)
                .then_some(value.value.as_str())
        })
    }
    let mut indexed = Vec::with_capacity(table.children.len());
    for range in &table.children {
        let index = range
            .name
            .to_ascii_lowercase()
            .strip_prefix("range")
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or_else(|| format!("invalid automatic-placement range {:?}", range.name))?;
        let maximum_mask_value = attribute(range, "threshold")
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or_else(|| format!("invalid automatic-placement threshold for {:?}", range.name))?;
        let mut choices = Vec::with_capacity(range.children.len());
        for entry in &range.children {
            let weight = attribute(entry, "weight")
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|weight| *weight != 0)
                .ok_or_else(|| {
                    format!("invalid automatic-placement weight for {:?}", entry.name)
                })?;
            let key = canonicalize_source_document_record_key(&entry.name);
            choices.push(BiomeAutomaticPlacementChoice {
                object: (key != "nothing").then(|| AssetId::from_key(&key)),
                weight,
            });
        }
        indexed.push((
            index,
            BiomeAutomaticPlacementRange {
                maximum_mask_value,
                choices,
            },
        ));
    }
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
