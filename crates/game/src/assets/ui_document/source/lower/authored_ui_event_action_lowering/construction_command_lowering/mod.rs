use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::{
    event_i32, required_bool, required_i8, required_tenths,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    invalid_at, parse_bool,
};
use crate::assets::ui_document::source::lower::canonical_source_value_resolution;
use openzt2_game_data::ui_document::action::construction::{
    UiBiomePaintClass, UiConstructionAction, UiConstructionActionRecord, UiFencePlacementMode,
    UiTankEditMode, UiTerrainEditMode,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind;
use std::io;

pub(super) fn lower_construction_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    current_stable_name: &str,
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
                definition: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
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
                definition: canonical_source_value_resolution::lower_optional_authored_semantic_key_to_asset_id(event.string.as_deref().or(event.value.as_deref())),
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
        "ZT_SET_AUTO_PLACEMENT_LIST" => Ok(UiActionRecord::Construction(UiConstructionActionRecord {
            trigger,
            action: UiConstructionAction::SetBiomeAutomaticPlacementVariation {
                variation: match canonicalize_source_document_record_key(current_stable_name).as_str() {
                    "foliage-mix" => BiomeAutomaticPlacementVariationKind::FoliageMix,
                    "foliage-mix-terrain" => BiomeAutomaticPlacementVariationKind::FoliageMixTerrain,
                    _ => return Err(invalid_at(input, format!("automatic-placement payload belongs to unsupported control {current_stable_name:?}"))),
                },
            },
        })),
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
