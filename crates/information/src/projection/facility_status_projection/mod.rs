use super::panel_binding_families::*;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::donations::donation_payment_types::DonationTotal;
use openzt2_zoo_state::economy::facility_economy_types::FacilityProfit;
use openzt2_zoo_state::economy::facility_economy_types::Inventory;
use openzt2_zoo_state::economy::facility_economy_types::MonthlyUpkeep;
use openzt2_zoo_state::economy::facility_economy_types::OperatingSinceDay;
use openzt2_zoo_state::economy::facility_economy_types::Price;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;
use openzt2_zoo_state::economy::service_types::FacilityReservations;

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_selected_facility_status_to_visible_information_panels(
    information_panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<FacilityFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    zoo_clock: Single<&ZooClock>,
    facility_subjects: Query<(
        Option<&Price>,
        Option<&ServiceFacility>,
        Option<&FacilityReservations>,
        Option<&Inventory>,
        Option<&MonthlyUpkeep>,
        Option<&FacilityProfit>,
        Option<&OperatingSinceDay>,
        Option<&DonationTotal>,
    )>,
    mut authored_integer_values: Query<
        (&UiValueBinding, &mut UiValue, Option<&mut Text>),
        FacilityField,
    >,
) {
    let zoo_clock = zoo_clock.into_inner();
    for (parent, inherited_visibility, fields) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((price, facility, reservations, inventory, upkeep, profit, opened, donations)) =
            facility_subjects.get(selection.0)
        else {
            continue;
        };

        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((property_binding, mut projected_value, text)) =
                authored_integer_values.get_mut(entity)
            else {
                continue;
            };
            let Some(next_value) = (match &property_binding.0 {
                UiIntegerPropertyBindingSource::FacilityPriceCents => price.map(|price| price.0 .0),
                UiIntegerPropertyBindingSource::FacilityCapacityUsed => facility.map(|_| {
                    i64::try_from(reservations.map_or(0, RelationshipTarget::len))
                        .unwrap_or(i64::MAX)
                }),
                UiIntegerPropertyBindingSource::FacilityCapacityTotal => {
                    facility.map(|facility| i64::from(facility.capacity))
                }
                UiIntegerPropertyBindingSource::FacilityInventoryCount => {
                    inventory.map(|inventory| i64::from(inventory.available))
                }
                UiIntegerPropertyBindingSource::FacilityInventoryCapacity => {
                    inventory.map(|inventory| i64::from(inventory.capacity))
                }
                UiIntegerPropertyBindingSource::FacilityUpkeepCentsPerMonth => {
                    upkeep.map(|upkeep| upkeep.0 .0)
                }
                UiIntegerPropertyBindingSource::FacilityProfitCents => {
                    profit.map(|profit| profit.total.0)
                }
                UiIntegerPropertyBindingSource::FacilityAverageProfitCents => {
                    profit.map(|profit| profit.average().0)
                }
                UiIntegerPropertyBindingSource::FacilityOperatingDays => {
                    opened.map(|opened| i64::from(zoo_clock.absolute_day.saturating_sub(opened.0)))
                }
                UiIntegerPropertyBindingSource::FacilityOperatingMonths => opened
                    .map(|opened| i64::from(zoo_clock.absolute_day.saturating_sub(opened.0) / 30)),
                UiIntegerPropertyBindingSource::DonationTotalCents => {
                    donations.map(|donations| donations.amount.0)
                }
                UiIntegerPropertyBindingSource::DonationCount => {
                    donations.map(|donations| i64::from(donations.count))
                }
                _ => None,
            }) else {
                continue;
            };
            if projected_value.0 == next_value
                && !projected_value.is_added()
                && text.as_ref().is_none_or(|text| !text.is_added())
            {
                continue;
            }
            projected_value.set_if_neq(UiValue(next_value));
            if let Some(mut text) = text {
                text.0.clear();
                if matches!(
                    property_binding.0,
                    UiIntegerPropertyBindingSource::FacilityPriceCents
                        | UiIntegerPropertyBindingSource::FacilityUpkeepCentsPerMonth
                        | UiIntegerPropertyBindingSource::FacilityProfitCents
                        | UiIntegerPropertyBindingSource::FacilityAverageProfitCents
                        | UiIntegerPropertyBindingSource::DonationTotalCents
                ) {
                    openzt2_zoo_state::economy::currency_formatting::write_whole_dollar_currency_with_thousands_separators(&mut text.0, next_value);
                } else {
                    use std::fmt::Write;
                    write!(text.0, "{next_value}").expect("writing to a String cannot fail");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
    use openzt2_zoo_state::economy::money_types::Money;

    #[test]
    fn facility_values_render_text_without_dirtying_settled_text() {
        let mut app = App::new();
        app.world_mut().spawn(ZooClock {
            fractional_tick: Default::default(),
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        });
        app.add_systems(
            Update,
            (
                super::super::panel_binding_families::bind_information_panel_fields,
                project_selected_facility_status_to_visible_information_panels,
            )
                .chain(),
        );
        let subject = app.world_mut().spawn(MonthlyUpkeep(Money(5000))).id();
        let owner = app.world_mut().spawn(SelectedSubject(subject)).id();
        let panel = app
            .world_mut()
            .spawn((InfoPanel, ChildOf(owner), InheritedVisibility::VISIBLE))
            .id();
        let text = app
            .world_mut()
            .spawn((
                UiDocumentOwner(panel),
                UiValueBinding(UiIntegerPropertyBindingSource::FacilityUpkeepCentsPerMonth),
                UiValue(0),
                Text::default(),
            ))
            .id();
        app.update();
        assert_eq!(app.world().get::<Text>(text).unwrap().0, "$50");
        let text_tick = app
            .world()
            .entity(text)
            .get_ref::<Text>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .entity(text)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            text_tick
        );
        app.world_mut()
            .entity_mut(subject)
            .insert(MonthlyUpkeep(Money(7500)));
        app.update();
        assert_eq!(app.world().get::<Text>(text).unwrap().0, "$75");
    }
}
