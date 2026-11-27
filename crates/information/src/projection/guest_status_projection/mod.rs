use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestEducation;
use openzt2_zoo_state::guests::guest_simulation_types::GuestEnergy;
use openzt2_zoo_state::guests::guest_simulation_types::GuestHunger;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRestroom;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSatisfaction;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSocial;
use openzt2_zoo_state::guests::guest_simulation_types::GuestThirst;
use openzt2_zoo_state::guests::guest_simulation_types::VisitTime;

use openzt2_interface_state::information::entity_selection_types::InfoPanel;

pub(crate) fn project_live_guest_status_to_visible_information_panels(
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
    mut values: Query<(
        &UiDocumentOwner,
        &UiValueBinding,
        &mut UiValue,
        Option<&mut Text>,
    )>,
) {
    for (panel_entity, panel, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Ok(status) = guests.get(panel.subject) else {
            continue;
        };
        for (owner, binding, mut value, text) in &mut values {
            if owner.0 != panel_entity {
                continue;
            }
            let next = match &binding.0 {
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
            value.set_if_neq(UiValue(next));
            // The cash field shows its value as text; nothing else renders it.
            if let (UiIntegerPropertyBindingSource::GuestCashCents, Some(mut text)) =
                (&binding.0, text)
            {
                let mut money = String::new();
                openzt2_zoo_state::economy::currency_formatting::write_whole_dollar_currency_with_thousands_separators(&mut money, next);
                if text.0 != money {
                    text.0 = money;
                }
            }
        }
    }
}
