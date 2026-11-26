use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource as Boolean, UiIntegerPropertyBindingSource as Integer,
};
use openzt2_game_data::ui_document::node_property_binding::{
    UiImagePropertyBindingSource, UiTextPropertyBindingSource,
};
use openzt2_interface_state::{
    information::entity_selection_types::InfoPanel,
    ui::authored_ui_node_projection_components::{
        UiDocumentMembers, UiDocumentOwner, UiValueBinding, UiVisibleBinding,
    },
};
use openzt2_ui::{
    authored_ui_image_content_binding::UiImageBinding,
    authored_ui_interaction_enabled_binding::UiEnabledBinding,
    authored_ui_selection_binding::UiSelectedBinding,
    authored_ui_text_content_binding::UiTextBinding,
};

macro_rules! binding_family {
    ($source:ident, $target:ident) => {
        #[derive(Component)]
        #[relationship(relationship_target = $target, allow_self_referential)]
        pub(crate) struct $source(Entity);

        #[derive(Component)]
        #[relationship_target(relationship = $source)]
        pub(crate) struct $target(Vec<Entity>);
    };
}

binding_family!(AnimalNeedFieldOf, AnimalNeedFields);
binding_family!(AnimalHealthValueFieldOf, AnimalHealthValueFields);
binding_family!(AnimalHandlingFieldOf, AnimalHandlingFields);
binding_family!(AnimalSummaryFieldOf, AnimalSummaryFields);
binding_family!(FacilityFieldOf, FacilityFields);
binding_family!(GuestFieldOf, GuestFields);
binding_family!(HabitatFieldOf, HabitatFields);
binding_family!(MaintenanceFieldOf, MaintenanceFields);
binding_family!(StaffFieldOf, StaffFields);
binding_family!(TransportFieldOf, TransportFields);
binding_family!(ShowFieldOf, ShowFields);
binding_family!(AnimalHealthVisibilityFieldOf, AnimalHealthVisibilityFields);
binding_family!(HabitatVisibilityFieldOf, HabitatVisibilityFields);
binding_family!(StaffVisibilityFieldOf, StaffVisibilityFields);
binding_family!(TransportVisibilityFieldOf, TransportVisibilityFields);
binding_family!(ShowVisibilityFieldOf, ShowVisibilityFields);
binding_family!(StaffDutyFieldOf, StaffDutyFields);
binding_family!(IdentityTextFieldOf, IdentityTextFields);
binding_family!(IdentityIconFieldOf, IdentityIconFields);

pub(super) type AnimalNeedField = (
    With<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type AnimalHealthValueField = (
    With<AnimalHealthValueFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type FacilityField = (
    With<FacilityFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type GuestField = (
    With<GuestFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type HabitatField = (
    With<HabitatFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type MaintenanceField = (
    With<MaintenanceFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type StaffField = (
    With<StaffFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<TransportFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type TransportField = (
    With<TransportFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<ShowFieldOf>,
);
pub(super) type ShowField = (
    With<ShowFieldOf>,
    Without<AnimalNeedFieldOf>,
    Without<AnimalHealthValueFieldOf>,
    Without<FacilityFieldOf>,
    Without<GuestFieldOf>,
    Without<HabitatFieldOf>,
    Without<MaintenanceFieldOf>,
    Without<StaffFieldOf>,
    Without<TransportFieldOf>,
);
pub(super) type AnimalHealthVisibilityField = (
    With<AnimalHealthVisibilityFieldOf>,
    Without<HabitatVisibilityFieldOf>,
    Without<StaffVisibilityFieldOf>,
    Without<TransportVisibilityFieldOf>,
    Without<ShowVisibilityFieldOf>,
);
pub(super) type HabitatVisibilityField = (
    With<HabitatVisibilityFieldOf>,
    Without<AnimalHealthVisibilityFieldOf>,
    Without<StaffVisibilityFieldOf>,
    Without<TransportVisibilityFieldOf>,
    Without<ShowVisibilityFieldOf>,
);
pub(super) type StaffVisibilityField = (
    With<StaffVisibilityFieldOf>,
    Without<AnimalHealthVisibilityFieldOf>,
    Without<HabitatVisibilityFieldOf>,
    Without<TransportVisibilityFieldOf>,
    Without<ShowVisibilityFieldOf>,
);
pub(super) type TransportVisibilityField = (
    With<TransportVisibilityFieldOf>,
    Without<AnimalHealthVisibilityFieldOf>,
    Without<HabitatVisibilityFieldOf>,
    Without<StaffVisibilityFieldOf>,
    Without<ShowVisibilityFieldOf>,
);
pub(super) type ShowVisibilityField = (
    With<ShowVisibilityFieldOf>,
    Without<AnimalHealthVisibilityFieldOf>,
    Without<HabitatVisibilityFieldOf>,
    Without<StaffVisibilityFieldOf>,
    Without<TransportVisibilityFieldOf>,
);
pub(super) type StaffDutyField = (With<StaffDutyFieldOf>,);

pub(crate) fn bind_information_panel_fields(
    mut commands: Commands,
    panels: Query<(), With<InfoPanel>>,
    new_panels: Query<&UiDocumentMembers, Added<InfoPanel>>,
    nodes: Query<(
        &UiDocumentOwner,
        Option<&UiValueBinding>,
        Option<&UiVisibleBinding>,
        Option<&UiSelectedBinding>,
        Option<&UiTextBinding>,
        Option<&UiImageBinding>,
        Option<&UiEnabledBinding>,
    )>,
    changed: Query<
        Entity,
        Or<(
            Changed<UiDocumentOwner>,
            Changed<UiValueBinding>,
            Changed<UiVisibleBinding>,
            Changed<UiSelectedBinding>,
            Changed<UiTextBinding>,
            Changed<UiImageBinding>,
            Changed<UiEnabledBinding>,
        )>,
    >,
    mut removed: (
        RemovedComponents<UiDocumentOwner>,
        RemovedComponents<UiValueBinding>,
        RemovedComponents<UiVisibleBinding>,
        RemovedComponents<UiSelectedBinding>,
        RemovedComponents<UiTextBinding>,
        RemovedComponents<UiImageBinding>,
        RemovedComponents<UiEnabledBinding>,
    ),
) {
    let mut work = bevy::ecs::entity::EntityHashSet::default();
    work.extend(
        changed
            .iter()
            .chain(removed.0.read())
            .chain(removed.1.read())
            .chain(removed.2.read())
            .chain(removed.3.read())
            .chain(removed.4.read())
            .chain(removed.5.read())
            .chain(removed.6.read()),
    );
    work.extend(new_panels.iter().flat_map(RelationshipTarget::iter));
    for node in work {
        let fields = nodes.get(node).ok();
        if fields.is_none() && commands.get_entity(node).is_err() {
            continue;
        }
        let mut entity = commands.entity(node);
        entity.remove::<(
            AnimalNeedFieldOf,
            AnimalHealthValueFieldOf,
            FacilityFieldOf,
            GuestFieldOf,
            HabitatFieldOf,
            MaintenanceFieldOf,
            StaffFieldOf,
            TransportFieldOf,
            ShowFieldOf,
        )>();
        entity.remove::<(
            AnimalHealthVisibilityFieldOf,
            HabitatVisibilityFieldOf,
            StaffVisibilityFieldOf,
            TransportVisibilityFieldOf,
            ShowVisibilityFieldOf,
        )>();
        entity.remove::<(StaffDutyFieldOf,)>();
        entity.remove::<(IdentityTextFieldOf, IdentityIconFieldOf)>();
        entity.remove::<AnimalHandlingFieldOf>();
        entity.remove::<AnimalSummaryFieldOf>();
        let Some((owner, value, visible, selected, text, image, enabled)) = fields else {
            continue;
        };
        let panel = panels.contains(owner.0);
        let health = |binding: &Boolean| {
            matches!(
                binding,
                Boolean::AnimalDiseased
                    | Boolean::AnimalUnderTreatment
                    | Boolean::AnimalTranquilized
                    | Boolean::AnimalEscaped
                    | Boolean::AnimalRampaging
                    | Boolean::AnimalDead
            )
        };
        if text.is_some_and(|text| {
            matches!(
                text.0,
                UiTextPropertyBindingSource::AnimalDiseaseName
                    | UiTextPropertyBindingSource::AnimalTreatmentName
            ) || !panel
                && matches!(
                    text.0,
                    UiTextPropertyBindingSource::SelectedEntityName
                        | UiTextPropertyBindingSource::SelectedEntityDefinitionName
                )
        }) || enabled.is_some_and(|enabled| health(&enabled.0))
            || (!panel && visible.is_some_and(|visible| health(&visible.0)))
        {
            entity.insert(AnimalSummaryFieldOf(owner.0));
        }
        if !panel {
            continue;
        }
        if value.is_some_and(|value| matches!(value.0, Integer::AnimalConservationStatus))
            || text.is_some_and(|text| {
                matches!(
                    text.0,
                    UiTextPropertyBindingSource::AnimalConservationStatus
                )
            })
            || visible.is_some_and(|visible| {
                matches!(
                    visible.0,
                    Boolean::AnimalPickupVisible
                        | Boolean::AnimalReleaseVisible
                        | Boolean::SelectedEntityCrated
                )
            })
            || enabled.is_some_and(|enabled| {
                matches!(
                    enabled.0,
                    Boolean::AnimalPickupEnabled | Boolean::SelectedEntityCrated
                )
            })
        {
            entity.insert(AnimalHandlingFieldOf(owner.0));
        }
        if text.is_some_and(|text| {
            matches!(
                &text.0,
                UiTextPropertyBindingSource::SelectedEntityName
                    | UiTextPropertyBindingSource::SelectedEntityDefinitionName
            )
        }) {
            entity.insert(IdentityTextFieldOf(owner.0));
        }
        if image.is_some_and(|image| {
            matches!(&image.0, UiImagePropertyBindingSource::SelectedEntityIcon)
        }) {
            entity.insert(IdentityIconFieldOf(owner.0));
        }
        if let Some(value) = value {
            match &value.0 {
                Integer::AnimalHungerQ16Permille
                | Integer::AnimalThirstQ16Permille
                | Integer::AnimalRestQ16Permille
                | Integer::AnimalPrivacyQ16Permille
                | Integer::AnimalSocialQ16Permille
                | Integer::AnimalExerciseQ16Permille
                | Integer::AnimalStimulationQ16Permille
                | Integer::AnimalEnvironmentQ16Permille
                | Integer::AnimalHygieneQ16Permille
                | Integer::AnimalHealthNeedQ16Permille
                | Integer::AnimalWelfarePermille => {
                    entity.insert(AnimalNeedFieldOf(owner.0));
                }
                Integer::AnimalVitalityPermille => {
                    entity.insert(AnimalHealthValueFieldOf(owner.0));
                }
                Integer::FacilityPriceCents
                | Integer::FacilityCapacityUsed
                | Integer::FacilityCapacityTotal
                | Integer::FacilityInventoryCount
                | Integer::FacilityInventoryCapacity
                | Integer::FacilityUpkeepCentsPerMonth
                | Integer::FacilityProfitCents
                | Integer::FacilityAverageProfitCents
                | Integer::FacilityOperatingDays
                | Integer::FacilityOperatingMonths
                | Integer::DonationTotalCents
                | Integer::DonationCount => {
                    entity.insert(FacilityFieldOf(owner.0));
                }
                Integer::GuestHungerPermille
                | Integer::GuestThirstPermille
                | Integer::GuestEnergyPermille
                | Integer::GuestRestroomPermille
                | Integer::GuestSocialPermille
                | Integer::GuestSatisfactionPermille
                | Integer::GuestEducationPermille
                | Integer::GuestVisitTicks
                | Integer::GuestCashCents => {
                    entity.insert(GuestFieldOf(owner.0));
                }
                Integer::HabitatAreaMilliSquareMetres
                | Integer::HabitatLandMilliSquareMetres
                | Integer::HabitatWaterMilliSquareMetres
                | Integer::HabitatBiomeAreaMilliSquareMetres { .. }
                | Integer::TankWaterQualityPermille
                | Integer::TankUsedLitres
                | Integer::TankRequiredLitres => {
                    entity.insert(HabitatFieldOf(owner.0));
                }
                Integer::FacilityWasteUnits
                | Integer::FacilityWasteCapacity
                | Integer::FacilityTrashLevel
                | Integer::LitterAmountUnits
                | Integer::HabitatWasteAmountUnits
                | Integer::ZooCleanlinessPermille => {
                    entity.insert(MaintenanceFieldOf(owner.0));
                }
                Integer::StaffWageCentsPerDay
                | Integer::StaffMonthsEmployed
                | Integer::StaffJobUrgency
                | Integer::StaffJobRemainingTicks => {
                    entity.insert(StaffFieldOf(owner.0));
                }
                Integer::TransportStationQueued
                | Integer::TransportStationOccupied
                | Integer::TransportStationCapacity
                | Integer::TransportVehicleOccupied
                | Integer::TransportVehicleSeats
                | Integer::TransportTripScoreMilli => {
                    entity.insert(TransportFieldOf(owner.0));
                }
                Integer::ShowScheduleCount
                | Integer::ShowScheduleCapacity
                | Integer::ShowRemainingTicks
                | Integer::ShowSlotDurationTicks { .. } => {
                    entity.insert(ShowFieldOf(owner.0));
                }
                _ => {}
            }
        }
        if let Some(visible) = visible {
            match &visible.0 {
                Boolean::AnimalDiseased
                | Boolean::AnimalUnderTreatment
                | Boolean::AnimalTranquilized
                | Boolean::AnimalEscaped
                | Boolean::AnimalRampaging
                | Boolean::AnimalDead => {
                    entity.insert(AnimalHealthVisibilityFieldOf(owner.0));
                }
                Boolean::HabitatBreached | Boolean::IsTank => {
                    entity.insert(HabitatVisibilityFieldOf(owner.0));
                }
                Boolean::StaffAssigned | Boolean::StaffWorking => {
                    entity.insert(StaffVisibilityFieldOf(owner.0));
                }
                Boolean::TransportCircuitClosed
                | Boolean::TransportStationRunsSkyTrack { .. }
                | Boolean::TransportWaiting
                | Boolean::TransportRiding => {
                    entity.insert(TransportVisibilityFieldOf(owner.0));
                }
                Boolean::ShowScheduled | Boolean::ShowRunning => {
                    entity.insert(ShowVisibilityFieldOf(owner.0));
                }
                _ => {}
            }
        }
        if let Some(selected) = selected {
            match &selected.0 {
                Boolean::WorkerDutyAssigned(_) => {
                    entity.insert(StaffDutyFieldOf(owner.0));
                }
                _ => {}
            }
        }
    }
}
