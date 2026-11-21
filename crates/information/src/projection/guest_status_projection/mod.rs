use super::panel_binding_families::*;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

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

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_live_guest_status_to_visible_information_panels(
    panels: Query<(&ChildOf, Ref<InheritedVisibility>, Option<Ref<GuestFields>>), With<InfoPanel>>,
    selections: Query<Ref<SelectedSubject>>,
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
    mut values: Query<(&UiValueBinding, &mut UiValue), GuestField>,
) {
    for (parent, visible, fields) in &panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !visible.get() {
            continue;
        }
        let Ok(status) = guests.get(selection.0) else {
            continue;
        };
        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((binding, mut value)) = values.get_mut(entity) else {
                continue;
            };
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
        }
    }
}
