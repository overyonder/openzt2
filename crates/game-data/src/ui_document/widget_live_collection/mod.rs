use serde::{Deserialize, Serialize};

/// Collection displayed by a list widget.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiWidgetLiveCollectionSource {
    #[default]
    Unbound,
    DisplayResolutions,
    GraphicsDetailSettings,
    ProfileIndex,
    WorldChoices,
    WorldChoicePrototype,
    Campaigns,
    CampaignScenarios,
    SelectedEntityInventory,
    SelectedAnimalBasicNeeds,
    SelectedAnimalAdvancedNeeds,
    /// The selected animal's happiness, health and, when pregnant, pregnancy.
    SelectedAnimalStates,
    /// The selected animal's, guest's or staff member's current thoughts,
    /// newest first.
    SelectedEntityThoughts,
    /// The selected guest's happiness and needs.
    SelectedGuestNeeds,
    /// The selected guest's favourite animal.
    SelectedGuestFavorites,
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
    SavedGameSlots,
}
