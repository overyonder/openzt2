use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;
use openzt2_zoo_state::maintenance::maintenance_types::AnimalHabitatWaste;
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWastePresentationLevel;
use openzt2_zoo_state::maintenance::maintenance_types::LooseLitterWaste;
use openzt2_zoo_state::maintenance::maintenance_types::ZooCleanlinessPermille;

use openzt2_interface_state::information::entity_selection_types::InfoPanel;

/// Updates maintenance information for the selected entity.
pub(crate) fn project_live_maintenance_status_to_visible_information_panels(
    cleanliness: Res<ZooCleanlinessPermille>,
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
    subjects: Query<(
        Option<Ref<FacilityContainedWaste>>,
        Option<Ref<LooseLitterWaste>>,
        Option<Ref<AnimalHabitatWaste>>,
        Option<Ref<WaterQuality>>,
    )>,
    mut values: Query<(
        &UiDocumentOwner,
        &UiValueBinding,
        &mut UiValue,
        &mut Visibility,
    )>,
) {
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok((container, litter, habitat_waste, water_quality)) = subjects.get(panel.subject)
        else {
            continue;
        };
        if !panel.is_changed()
            && !cleanliness.is_changed()
            && container.as_ref().is_none_or(|value| !value.is_changed())
            && litter.as_ref().is_none_or(|value| !value.is_changed())
            && habitat_waste
                .as_ref()
                .is_none_or(|value| !value.is_changed())
            && water_quality
                .as_ref()
                .is_none_or(|value| !value.is_changed())
        {
            continue;
        }
        for (owner, binding, mut value, mut visibility) in &mut values {
            if owner.0 != panel_entity {
                continue;
            }
            let next = match &binding.0 {
                UiIntegerPropertyBindingSource::FacilityWasteUnits => container
                    .as_ref()
                    .map(|container| i64::from(container.contained_waste_units)),
                UiIntegerPropertyBindingSource::FacilityWasteCapacity => container
                    .as_ref()
                    .map(|container| i64::from(container.contained_waste_capacity_units)),
                UiIntegerPropertyBindingSource::FacilityTrashLevel => {
                    container.as_ref().map(|container| {
                        match FacilityContainedWastePresentationLevel::
                            calculate_from_facility_contained_waste(**container)
                        {
                            // The shipped UIMultiIcon order is full, two-thirds,
                            // one-third, empty. UiValue indexes that immutable
                            // UI document table directly.
                            FacilityContainedWastePresentationLevel::Full => 0,
                            FacilityContainedWastePresentationLevel::TwoThirds => 1,
                            FacilityContainedWastePresentationLevel::OneThird => 2,
                            FacilityContainedWastePresentationLevel::Empty => 3,
                        }
                    })
                }
                UiIntegerPropertyBindingSource::LitterAmountUnits => litter
                    .as_ref()
                    .map(|litter| i64::from(litter.uncontained_waste_units)),
                UiIntegerPropertyBindingSource::HabitatWasteAmountUnits => habitat_waste
                    .as_ref()
                    .map(|waste| i64::from(waste.uncontained_waste_units)),
                UiIntegerPropertyBindingSource::ZooCleanlinessPermille => {
                    visibility.set_if_neq(if cleanliness.0.is_some() {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                    cleanliness.0.map(i64::from)
                }
                UiIntegerPropertyBindingSource::TankWaterQualityPermille => {
                    water_quality.as_ref().map(|quality| i64::from(quality.0))
                }
                _ => None,
            };
            if let Some(next) = next {
                if value.0 != next {
                    value.0 = next;
                }
            }
        }
    }
}
