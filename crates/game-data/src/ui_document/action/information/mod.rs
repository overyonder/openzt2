//! Information views, entity lists, settings, encyclopedia, sorting, graphs, and guest-view actions.

use super::UiTrigger;
use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiInformationActionRecord {
    pub trigger: UiTrigger,
    pub action: UiInformationAction,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationViewCategory {
    Animals,
    Guests,
    Buildings,
    Entrances,
    Fences,
    Curbs,
    ZooWalls,
    Foliage,
    Shows,
    Tanks,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationListCategory {
    Animals,
    Guests,
    Staff,
    Buildings,
    DonationBoxes,
    Vehicles,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationListCriterion {
    Type,
    Name,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationSortField {
    Name,
    Type,
    MonthsOpen,
    Profit,
    AverageProfit,
    CurrentCapacity,
    TotalCapacity,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationSettingAction {
    Refresh,
    HighestDetail,
    HighDetail,
    MediumDetail,
    CustomDetail,
    Fullscreen,
    Windowed,
    FreeMouseLook(bool),
    MessageOfTheDay(bool),
    Accept,
    Back,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationGraphSeries {
    ZooValue,
    ZooProfit,
    DonationIncome,
    AdmissionUsers,
    Fame,
    Members,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InformationGraphType {
    Bar,
    Line,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiInformationAction {
    ExportOverviewMap {
        return_control: AssetId,
    },
    RenameSelected,
    RenameZoo,
    SelectNextDiseasedAnimal,
    SelectNextRampagingAnimal,
    SelectEntityFromSource,
    SetViewFilter {
        category: InformationViewCategory,
        visible: bool,
    },
    PopulateEntityList {
        list: AssetId,
        category: InformationListCategory,
    },
    ApplySetting {
        setting: InformationSettingAction,
    },
    OpenEncyclopediaEntry {
        entry: AssetId,
    },
    OpenContextEncyclopediaEntry,
    SortEntityList {
        list: AssetId,
        criterion: InformationListCriterion,
    },
    ZoopediaBack,
    SortEntityListByNeed {
        list: AssetId,
        descending: bool,
    },
    ZoopediaForward,
    SortAnimalsByHappiness {
        list: AssetId,
        descending: bool,
    },
    SortAnimalsByPregnancy {
        list: AssetId,
        descending: bool,
    },
    SortGuestsByFavouriteAnimal {
        list: AssetId,
    },
    PopulateEntityEditorDataRoots,
    SelectGraph {
        graph: InformationGraphSeries,
    },
    SelectGraphType {
        graph_type: InformationGraphType,
    },
    SortEntityListDirected {
        list: AssetId,
        field: InformationSortField,
        descending: bool,
    },
    SetResearchFilter {
        list: AssetId,
        unlocked_only: bool,
    },
    SetCatalogueKindFilter {
        list: AssetId,
        kind: AssetId,
    },
    ClearTypeFilter {
        list: AssetId,
    },
}
