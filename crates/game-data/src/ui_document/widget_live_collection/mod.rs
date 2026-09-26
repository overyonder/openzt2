use serde::{Deserialize, Serialize};

/// Collection displayed by a list widget.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiWidgetLiveCollectionSource {
    #[default]
    Unbound,
    DisplayResolutions,
    ProfileIndex,
    WorldChoices,
    WorldChoicePrototype,
    Campaigns,
    CampaignScenarios,
    SelectedEntityInventory,
    SelectedAnimalBasicNeeds,
    SelectedAnimalAdvancedNeeds,
    ScenarioObjectives,
    ZoopediaTableOfContents,
    OverviewLayers,
    PhotoCameraRoll,
    PhotoAlbums,
    FinanceBalanceSheetCategories,
    FinanceBalanceSheetMonths,
    FinanceBalanceSheetMonthColumns,
    FinanceBalanceSheetMonthValues,
    FinanceBuildings,
    FinanceDonationBoxes,
    FinanceDonationsBySpecies,
    FinanceTourDonations,
    FinanceShowDonations,
}
