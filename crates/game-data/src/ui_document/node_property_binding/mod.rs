use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiNodePropertyBinding {
    Visibility(UiBooleanPropertyBindingSource),
    InteractionEnabled(UiBooleanPropertyBindingSource),
    TextContent(UiTextPropertyBindingSource),
    ImageContent(UiImagePropertyBindingSource),
    IntegerValue(UiIntegerPropertyBindingSource),
    MinimumIntegerValue(i64),
    MaximumIntegerValue(i64),
    SelectionState(UiBooleanPropertyBindingSource),
    TranquilizerHeadsUpDisplaySlot(UiTranquilizerHeadsUpDisplaySlotBinding),
    ShellPresentationSlot(UiShellPresentationSlotBinding),
    /// Visibility follows the canonical simulation pause fact. Source lowering
    /// derives this from authored pause/show and unpause/hide action pairs.
    SimulationPausedVisibility,
}

/// Application-shell presentation slots.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiShellPresentationSlotBinding {
    ExitConfirmation,
}

/// Controls in the first-person tranquilizer overlay.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiTranquilizerHeadsUpDisplaySlotBinding {
    Screen,
    TargetName,
    TargetImage,
    DistanceLabel,
    DistanceText,
    OutOfRangeLabel,
    MisfireIndicator,
    Reticle,
    ChargeBar,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiBooleanPropertyBindingSource {
    Constant(bool),
    HasSelectedEntity,
    SelectedInformationPanelSubjectUsesSection {
        section: AssetId,
    },
    SelectedEntitySellable,
    ProfilePresent,
    SelectedWorldAdjustableCash,
    SelectedWorldUnlimitedCash,
    SelectedWorldDifficulty,
    AnimalDiseased,
    AnimalUnderTreatment,
    AnimalTranquilized,
    AnimalEscaped,
    AnimalRampaging,
    AnimalDead,
    AnimalPickupVisible,
    AnimalReleaseVisible,
    AnimalPickupEnabled,
    SelectedEntityCrated,
    StaffAssigned,
    StaffWorking,
    WorkerCleansFilters,
    WorkerCleansRecycling,
    WorkerEmptiesTrash,
    WorkerSweepsTrash,
    FacilityOpen,
    HabitatBreached,
    IsTank,
    ShowScheduled,
    ShowRunning,
    /// The most recent first-person training trace was rejected.
    TrainingAttemptRejected,
    TransportCircuitClosed,
    TransportWaiting,
    TransportRiding,
    ScenarioRunning,
    ScenarioObjectiveSatisfied {
        objective: AssetId,
    },
    DefinitionUnlocked {
        definition: AssetId,
    },
    ResearchActive,
    CatalogueSelectedEntryRequiresResearch,
    CatalogueSelectedEntryAvailable,
    AwardEarned {
        award: AssetId,
    },
    CatalogueRowVisible {
        row: u16,
    },
    CatalogueEntryUnlocked {
        row: u16,
    },
    CatalogueEntryAffordable {
        row: u16,
    },
    CatalogueSelectedEntryHasMonthlyUpkeep,
    CatalogueSelectedEntryHasBiomeAndLocation,
    CatalogueSelectedEntryIsAnimal,
    CatalogueSelectedEntryPlacementCanRotate,
    CatalogueSelectedEntrySellsItems,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiTextPropertyBindingSource {
    Localized {
        key: AssetId,
        format: AssetId,
    },
    SelectedEntityName,
    ZooName,
    SelectedEntityDefinitionName,
    SelectedEntitySaleConfirmation,
    SelectedEntitySaleRefund {
        format: AssetId,
        omit_fractional_currency_cents: bool,
    },
    AnimalWelfareBand,
    AnimalHealthStatus,
    AnimalConservationStatus,
    AnimalDiseaseName,
    AnimalTreatmentName,
    GuestPhase,
    StaffRoleName,
    StaffAssignmentName,
    StaffCurrentJobName,
    FacilityServiceName,
    HabitatPrimaryBiomeName,
    HabitatBiomeName {
        row: u16,
    },
    ShowStateName,
    ShowCurrentTrickName,
    ShowSlotTrickName {
        slot: u16,
    },
    TrainingAttemptScore {
        format: AssetId,
    },
    TransportCircuitName,
    TransportStationName,
    TransportVehicleName,
    TransportTripState,
    ZooCash {
        positive_format: AssetId,
        negative_format: AssetId,
    },
    ZooAdmissionPrice,
    ZooDonationSummaryDonationCount {
        category: Option<AssetId>,
        format: AssetId,
    },
    ZooDonationSummaryDonationAmount {
        category: Option<AssetId>,
        positive_format: AssetId,
    },
    /// Signed cost of the current construction preview. The authored
    /// cursor-money fragment owns the exact money and localized presentation
    /// policy rather than relying on the generic zoo-balance formatter.
    ConstructionPreviewCost {
        positive_format: AssetId,
        negative_format: AssetId,
        no_cents: bool,
        no_minus: bool,
    },
    ZooFame {
        format: AssetId,
    },
    ZooRating {
        format: AssetId,
    },
    ZooGuestCount {
        format: AssetId,
    },
    /// The running application's build version. The source reused its
    /// date-display formatter token for this main-menu field, so source lowering
    /// must distinguish the authored document role before runtime.
    ApplicationVersion,
    ProfileName,
    /// Localized heading for the selected campaign, challenge, or freeform
    /// shell mode.
    PlayModeHeading,
    SelectedWorldName,
    SelectedWorldLocation,
    SelectedWorldBiome,
    SelectedTerrainBiomeName,
    SelectedWorldSize,
    SelectedWorldDescription,
    SelectedWorldStartingCash,
    /// Localized overview of the active scenario, shown by the goal panel's
    /// authored Introduction presentation.
    ScenarioDescription,
    ScenarioObjectiveText {
        objective: AssetId,
    },
    ScenarioObjectiveStatus {
        objective: AssetId,
    },
    ScenarioObjectiveProgress {
        objective: AssetId,
        format: AssetId,
    },
    ScenarioTimeRemaining {
        format: AssetId,
    },
    ResearchName,
    ResearchProgress {
        format: AssetId,
    },
    LatestAwardName,
    CatalogueEntryName {
        row: u16,
    },
    CatalogueEntryPrice {
        row: u16,
        format: AssetId,
        omit_fractional_currency_cents: bool,
    },
    CatalogueEntryResearchPrice {
        format: AssetId,
        omit_fractional_currency_cents: bool,
    },
    CatalogueEntryUpkeep {
        row: u16,
        format: AssetId,
        omit_fractional_currency_cents: bool,
    },
    ZoopediaTitle,
    ZoopediaBody,
    /// Number printed on one side of the currently open photo-album spread.
    PhotoAlbumPageNumber {
        offset: u8,
    },
    /// Player-visible name of the active photo album.
    PhotoAlbumName,
    PhotoAlbumCaption {
        slot: u8,
    },
    /// Camera-roll occupancy and its authored fixed capacity.
    PhotoFilmCount,
    FinanceBalanceSheetCategoryLabel,
    FinanceBalanceSheetCategoryValue,
    FinanceBalanceSheetMonthName,
    /// The current simulation calendar date. Presentation is locale-owned;
    /// no source formatter identity reaches the runtime.
    ZooDate,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiImagePropertyBindingSource {
    Constant { texture: AssetId },
    SelectedWorldThumbnail,
    SelectedEntityIcon,
    ProfilePortrait,
    ScenarioObjectiveIcon { objective: AssetId },
    CatalogueEntryIcon { row: u16 },
    CatalogueEntryBiomeIcon,
    CatalogueEntryLocationIcon,
    CatalogueEntryPreview,
    ZoopediaImage,
    PhotoAlbumPicture { index: u8 },
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiIntegerPropertyBindingSource {
    Constant(i64),
    AnimalHungerQ16Permille,
    AnimalThirstQ16Permille,
    AnimalRestQ16Permille,
    AnimalPrivacyQ16Permille,
    AnimalSocialQ16Permille,
    AnimalExerciseQ16Permille,
    AnimalStimulationQ16Permille,
    AnimalEnvironmentQ16Permille,
    AnimalHygieneQ16Permille,
    AnimalHealthNeedQ16Permille,
    AnimalWelfarePermille,
    AnimalVitalityPermille,
    CatalogueEntryConservationStatus,
    AnimalConservationStatus,
    GuestHungerPermille,
    GuestThirstPermille,
    GuestEnergyPermille,
    GuestRestroomPermille,
    GuestSocialPermille,
    GuestSatisfactionPermille,
    GuestEducationPermille,
    GuestVisitTicks,
    GuestCashCents,
    StaffWageCentsPerDay,
    StaffMonthsEmployed,
    StaffJobUrgency,
    StaffJobRemainingTicks,
    FacilityPriceCents,
    FacilityCapacityUsed,
    FacilityCapacityTotal,
    FacilityConditionPermille,
    FacilityWasteUnits,
    FacilityWasteCapacity,
    FacilityTrashLevel,
    FacilityInventoryCount,
    FacilityInventoryCapacity,
    FacilityUpkeepCentsPerMonth,
    FacilityProfitCents,
    FacilityAverageProfitCents,
    FacilityOperatingDays,
    FacilityOperatingMonths,
    DonationTotalCents,
    DonationCount,
    DonationAverageCents,
    LitterAmountUnits,
    HabitatWasteAmountUnits,
    ZooCleanlinessPermille,
    HabitatAreaMilliSquareMetres,
    HabitatLandMilliSquareMetres,
    HabitatWaterMilliSquareMetres,
    HabitatBiomeAreaMilliSquareMetres { row: u16 },
    TankWaterQualityPermille,
    TankUsedLitres,
    TankRequiredLitres,
    ShowScheduleCount,
    ShowScheduleCapacity,
    ShowRemainingTicks,
    ShowSlotDurationTicks { slot: u16 },
    TrainingAttemptScoreMilli,
    TransportStationQueued,
    TransportStationOccupied,
    TransportStationCapacity,
    TransportVehicleOccupied,
    TransportVehicleSeats,
    TransportTripScoreMilli,
    ZooCashCents,
    ZooAdmissionCents,
    ZooFameHalfStars,
    ZooRatingPermille,
    ZooGuestCount,
    EndangeredBirthCount,
    ScenarioObjectiveCurrent { objective: AssetId },
    ScenarioObjectiveTarget { objective: AssetId },
    ScenarioTimeRemainingTicks,
    ResearchElapsedTicks,
    SelectedCatalogueResearchProgressBasisPoints,
    ResearchRequiredTicks,
    AwardEarnedTick { award: AssetId },
    CatalogueEntryPriceCents { row: u16 },
    CataloguePage,
    InformationListAnimalHappinessIcon,
    InformationListAnimalHealthIcon,
    InformationListAnimalPregnancyIcon,
    InformationListGuestHappinessIcon,
    InformationListStaffAssignmentCount,
}
