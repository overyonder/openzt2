//! Construction, placement, terrain, biome, fence, path, track, and tank-editing actions.

use super::UiTrigger;
use crate::world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiConstructionActionRecord {
    pub trigger: UiTrigger,
    pub action: UiConstructionAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiConstructionAction {
    SelectInspectionTool,
    SellSelected,
    SellPhysicalObject {
        definition: AssetId,
    },
    MoveSelected,
    RotateSelected {
        steps: i8,
    },
    SetTerrainCursorSize {
        direction: i32,
    },
    SetPlacementObject {
        definition: AssetId,
    },
    Undo,
    SetSingleObjectPlacement,
    CrateSelected,
    SetBiome {
        biome: AssetId,
    },
    SetBiomeSurface {
        surface: UiBiomeSurface,
    },
    SetBiomePaintTrees {
        enabled: bool,
    },
    SetBiomeAutomaticPlacementVariation {
        variation: BiomeAutomaticPlacementVariationKind,
    },
    SetFencePlacementMode {
        mode: UiFencePlacementMode,
    },
    SetTankEditMode {
        mode: UiTankEditMode,
    },
    SetTerrainEditMode {
        mode: UiTerrainEditMode,
    },
    SetTerrainFlattenHeight {
        height_decimetres: i32,
    },
    SetTerrainFlattenSpeed {
        strength_tenths_per_second: i32,
    },
    SetElevatedPathPlacementMode {
        grade_steps: i8,
    },
    AdjustElevatedPathHeight {
        steps: i8,
    },
    AdjustSkyTowerHeight {
        steps: i8,
    },
    SetBiomePaintClass {
        detail_class: UiBiomePaintClass,
        enabled: bool,
    },
    Eyedropper,
    UncrateSelected,
    EnterPlacementMode,
    EnterBiomeEditing,
    EnterTerrainEditing,
    EnterDeletionMode,
    EnterTankEditing,
    EnterFencePlacementMode,
    EnterPathPlacementMode,
    EnterElevatedPathPlacementMode,
    EnterElevatedCurbPlacementMode,
    EnterSkyTrackPlacementMode,
    EnterGroundTrackPlacementMode,
    ConfirmPlacement {
        confirmed: bool,
    },
    ResolveConfirmation {
        kind: UiConstructionConfirmation,
        confirmed: bool,
    },
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiConstructionConfirmation {
    ZooGatePathDeletion,
    TankDeletion,
    TankMerge,
    TankSplit,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiFencePlacementMode {
    RubberBand,
    Rectangle,
    Octagon,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTankEditMode {
    RaiseWall,
    LowerWall,
    RaiseFloor,
    LowerFloor,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTerrainEditMode {
    Hill,
    Valley,
    Flatten,
    Smooth,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiBiomeSurface {
    DeepWater,
    ShallowWater,
    Ground,
    MixedGround,
    GroundCover,
    FoliageMix,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiBiomePaintClass {
    Foliage,
    Rocks,
    All,
}
