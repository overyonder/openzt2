use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use crate::plugins::{
    economy::facility_economy_types::Wallet,
    guests::guest_simulation_types::{
        Guest, GuestEducation, GuestEnergy, GuestHunger, GuestRestroom, GuestSatisfaction,
        GuestSocial, GuestThirst, VisitTime,
    },
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_live_guest_status_to_visible_information_panels(
    panels: Query<(Entity, Ref<InfoPanel>, &InheritedVisibility)>,
    guests: Query<
        (
            Ref<GuestHunger>,
            Ref<GuestThirst>,
            Ref<GuestEnergy>,
            Ref<GuestRestroom>,
            Ref<GuestSocial>,
            Ref<GuestSatisfaction>,
            Ref<GuestEducation>,
            Ref<VisitTime>,
            Ref<Wallet>,
        ),
        With<Guest>,
    >,
    mut values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
) {
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok(status) = guests.get(panel.subject) else {
            continue;
        };
        for (owner, binding, mut value) in &mut values {
            if owner.0 != panel_entity {
                continue;
            }
            value.0 = match &binding.0 {
                UiIntegerPropertyBindingSource::GuestHungerPermille => i64::from(status.0.value),
                UiIntegerPropertyBindingSource::GuestThirstPermille => i64::from(status.1.value),
                UiIntegerPropertyBindingSource::GuestEnergyPermille => i64::from(status.2.value),
                UiIntegerPropertyBindingSource::GuestRestroomPermille => i64::from(status.3.value),
                UiIntegerPropertyBindingSource::GuestSocialPermille => i64::from(status.4.value),
                UiIntegerPropertyBindingSource::GuestSatisfactionPermille => i64::from(status.5 .0),
                UiIntegerPropertyBindingSource::GuestEducationPermille => i64::from(status.6 .0),
                UiIntegerPropertyBindingSource::GuestVisitTicks => {
                    i64::try_from(status.7.elapsed_ticks).unwrap_or(i64::MAX)
                }
                UiIntegerPropertyBindingSource::GuestCashCents => status.8 .0 .0,
                _ => continue,
            };
        }
    }
}
