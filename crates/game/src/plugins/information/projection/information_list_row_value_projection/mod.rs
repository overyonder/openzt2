use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use crate::plugins::{
    animal_health::types::Disease,
    animal_lifecycle::types::Pregnancy,
    animal_welfare::types::CumulativeNeedPoints,
    donations::donation_payment_types::DonationTotal,
    economy::{
        facility_economy_types::{FacilityProfit, OperatingSinceDay, ServiceFacility},
        finance_table_ui::write_whole_dollar_currency_with_thousands_separators,
    },
    guests::guest_simulation_types::GuestSatisfaction,
    simulation_time::simulation_clock_types::ZooClock,
    staff::staff_assignment_types::StaffAssignment,
    transport_tours::transport_vehicle_types::TransportVehicle,
    ui::authored_ui_node_projection_components::{UiValue, UiValueBinding},
};

use super::{
    super::entity_selection_types::InformationEntitySource,
    information_list_row_identity_projection::find_information_list_row_source_in_ancestors,
};

/// Updates information-list values from the referenced entities.
pub(in crate::plugins::information) fn project_live_world_subject_values_to_information_list_rows(
    information_entity_sources: Query<Ref<InformationEntitySource>>,
    parent_relationships: Query<&ChildOf>,
    animal_need_points: Query<&CumulativeNeedPoints>,
    animal_diseases: Query<(), With<Disease>>,
    animal_pregnancies: Query<(), With<Pregnancy>>,
    guest_satisfaction: Query<&GuestSatisfaction>,
    staff_assignments: Query<&StaffAssignment>,
    service_facilities: Query<&ServiceFacility>,
    facility_operating_dates: Query<&OperatingSinceDay>,
    facility_profits: Query<&FacilityProfit>,
    donation_totals: Query<&DonationTotal>,
    transport_vehicles: Query<&TransportVehicle>,
    zoo_clock: Res<ZooClock>,
    mut bound_values: Query<(Entity, &UiValueBinding, &mut UiValue, Option<&mut Text>)>,
) {
    for (value_entity, value_binding, mut projected_value, text) in &mut bound_values {
        let Some((world_subject, _)) = find_information_list_row_source_in_ancestors(
            value_entity,
            &information_entity_sources,
            &parent_relationships,
        ) else {
            continue;
        };
        let Some((next_value, currency)) = project_information_list_value(
            &value_binding.0,
            world_subject,
            &animal_need_points,
            &animal_diseases,
            &animal_pregnancies,
            &guest_satisfaction,
            &staff_assignments,
            &service_facilities,
            &facility_operating_dates,
            &facility_profits,
            &donation_totals,
            &transport_vehicles,
            zoo_clock.absolute_day,
        ) else {
            continue;
        };
        projected_value.0 = next_value;
        if let Some(mut text) = text {
            text.0.clear();
            if currency {
                write_whole_dollar_currency_with_thousands_separators(&mut text.0, next_value);
            } else {
                use std::fmt::Write;
                let _ = write!(text.0, "{next_value}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn project_information_list_value(
    binding: &UiIntegerPropertyBindingSource,
    subject: Entity,
    animal_need_points: &Query<&CumulativeNeedPoints>,
    animal_diseases: &Query<(), With<Disease>>,
    animal_pregnancies: &Query<(), With<Pregnancy>>,
    guest_satisfaction: &Query<&GuestSatisfaction>,
    staff_assignments: &Query<&StaffAssignment>,
    service_facilities: &Query<&ServiceFacility>,
    facility_operating_dates: &Query<&OperatingSinceDay>,
    facility_profits: &Query<&FacilityProfit>,
    donation_totals: &Query<&DonationTotal>,
    transport_vehicles: &Query<&TransportVehicle>,
    absolute_day: u32,
) -> Option<(i64, bool)> {
    let value = match binding {
        UiIntegerPropertyBindingSource::InformationListAnimalHappinessIcon => animal_need_points
            .get(subject)
            .map(|points| i64::from(points.good >= points.bad))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::InformationListAnimalHealthIcon => {
            i64::from(animal_diseases.get(subject).is_err())
        }
        UiIntegerPropertyBindingSource::InformationListAnimalPregnancyIcon => {
            i64::from(animal_pregnancies.get(subject).is_ok())
        }
        UiIntegerPropertyBindingSource::InformationListGuestHappinessIcon => guest_satisfaction
            .get(subject)
            .map(|satisfaction| i64::from(satisfaction.0 >= 500))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::InformationListStaffAssignmentCount => staff_assignments
            .get(subject)
            .map(|assignment| {
                i64::from(assignment.area.is_some()) + i64::from(assignment.target.is_some())
            })
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::FacilityOperatingMonths => facility_operating_dates
            .get(subject)
            .map(|opened| i64::from(absolute_day.saturating_sub(opened.0) / 30))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::FacilityCapacityUsed => service_facilities
            .get(subject)
            .map(|facility| i64::from(facility.occupied))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::FacilityProfitCents => facility_profits
            .get(subject)
            .ok()
            .map(|profit| profit.total.0)
            .or_else(|| {
                donation_totals
                    .get(subject)
                    .ok()
                    .map(|total| total.amount.0)
            })
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::FacilityAverageProfitCents => facility_profits
            .get(subject)
            .ok()
            .map(|profit| profit.average().0)
            .or_else(|| {
                donation_totals.get(subject).ok().map(|total| {
                    (total.count != 0)
                        .then(|| total.amount.0 / i64::from(total.count))
                        .unwrap_or(0)
                })
            })
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::DonationTotalCents => donation_totals
            .get(subject)
            .map(|total| total.amount.0)
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::DonationCount => donation_totals
            .get(subject)
            .map(|total| i64::from(total.count))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::DonationAverageCents => donation_totals
            .get(subject)
            .map(|total| {
                (total.count != 0)
                    .then(|| total.amount.0 / i64::from(total.count))
                    .unwrap_or(0)
            })
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::TransportVehicleOccupied => transport_vehicles
            .get(subject)
            .map(|vehicle| i64::from(vehicle.occupied))
            .unwrap_or(0),
        UiIntegerPropertyBindingSource::TransportVehicleSeats => transport_vehicles
            .get(subject)
            .map(|vehicle| i64::from(vehicle.seats))
            .unwrap_or(0),
        _ => return None,
    };
    Some((
        value,
        matches!(
            binding,
            UiIntegerPropertyBindingSource::FacilityProfitCents
                | UiIntegerPropertyBindingSource::FacilityAverageProfitCents
                | UiIntegerPropertyBindingSource::DonationTotalCents
                | UiIntegerPropertyBindingSource::DonationAverageCents
        ),
    ))
}
