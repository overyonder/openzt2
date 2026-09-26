use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use crate::plugins::{
    donations::donation_payment_types::DonationTotal,
    economy::facility_economy_types::{
        FacilityProfit, Inventory, MonthlyUpkeep, OperatingSinceDay, Price, ServiceFacility,
    },
    simulation_time::simulation_clock_types::ZooClock,
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_selected_facility_status_to_visible_information_panels(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    zoo_clock: Res<ZooClock>,
    facility_subjects: Query<(
        Option<&Price>,
        Option<&ServiceFacility>,
        Option<&Inventory>,
        Option<&MonthlyUpkeep>,
        Option<&FacilityProfit>,
        Option<&OperatingSinceDay>,
        Option<&DonationTotal>,
    )>,
    mut authored_integer_values: Query<(
        &UiDocumentOwner,
        &UiValueBinding,
        &mut UiValue,
        Option<&mut Text>,
    )>,
) {
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let Ok((price, facility, inventory, upkeep, profit, opened, donations)) =
            facility_subjects.get(information_panel.subject)
        else {
            continue;
        };

        for (document_owner, property_binding, mut projected_value, text) in
            &mut authored_integer_values
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let Some(next_value) = (match &property_binding.0 {
                UiIntegerPropertyBindingSource::FacilityPriceCents => price.map(|price| price.0 .0),
                UiIntegerPropertyBindingSource::FacilityCapacityUsed => {
                    facility.map(|facility| i64::from(facility.occupied))
                }
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
                    crate::plugins::economy::finance_table_ui::write_whole_dollar_currency_with_thousands_separators(&mut text.0, next_value);
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
    use crate::plugins::economy::money_types::Money;

    #[test]
    fn facility_values_render_text_without_dirtying_settled_text() {
        let mut app = App::new();
        app.insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_systems(
            Update,
            project_selected_facility_status_to_visible_information_panels,
        );
        let subject = app.world_mut().spawn(MonthlyUpkeep(Money(5000))).id();
        let panel = app
            .world_mut()
            .spawn((InfoPanel { subject }, InheritedVisibility::VISIBLE))
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
