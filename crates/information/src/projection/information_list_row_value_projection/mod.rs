use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::ui_document::node_property_binding::UiIntegerPropertyBindingSource;

use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_lifecycle::types::Pregnancy;
use openzt2_zoo_state::animal_welfare::types::CumulativeNeedPoints;
use openzt2_zoo_state::donations::donation_payment_types::DonationTotal;
use openzt2_zoo_state::economy::currency_formatting::write_whole_dollar_currency_with_thousands_separators;
use openzt2_zoo_state::economy::facility_economy_types::FacilityProfit;
use openzt2_zoo_state::economy::facility_economy_types::OperatingSinceDay;
use openzt2_zoo_state::economy::service_types::FacilityReservations;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSatisfaction;
use openzt2_zoo_state::staff::staff_assignment_types::StaffAssignment;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use super::{visit_information_row, InformationRows};

#[derive(SystemParam)]
pub(crate) struct InformationValueInvalidation<'w, 's> {
    subjects: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<CumulativeNeedPoints>,
            Added<Disease>,
            Added<Pregnancy>,
            Changed<GuestSatisfaction>,
            Changed<StaffAssignment>,
            Changed<FacilityReservations>,
            Changed<OperatingSinceDay>,
            Changed<FacilityProfit>,
            Changed<DonationTotal>,
            Changed<TransportVehicle>,
        )>,
    >,
    removed: (
        RemovedComponents<'w, 's, CumulativeNeedPoints>,
        RemovedComponents<'w, 's, Disease>,
        RemovedComponents<'w, 's, Pregnancy>,
        RemovedComponents<'w, 's, GuestSatisfaction>,
        RemovedComponents<'w, 's, StaffAssignment>,
        RemovedComponents<'w, 's, FacilityReservations>,
        RemovedComponents<'w, 's, OperatingSinceDay>,
        RemovedComponents<'w, 's, FacilityProfit>,
        RemovedComponents<'w, 's, DonationTotal>,
        RemovedComponents<'w, 's, TransportVehicle>,
    ),
    row_sources_removed: RemovedComponents<
        'w,
        's,
        openzt2_interface_state::information::entity_selection_types::InformationEntitySource,
    >,
    rows_removed: RemovedComponents<
        'w,
        's,
        openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow,
    >,
    bindings: Query<
        'w,
        's,
        Entity,
        (
            With<UiValueBinding>,
            Or<(Changed<UiValueBinding>, Changed<ChildOf>, Added<Text>)>,
        ),
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    last_day: Local<'s, u32>,
}

/// Updates information-list values from the referenced entities.
pub(crate) fn project_live_world_subject_values_to_information_list_rows(
    information_entity_sources: InformationRows,
    children: Query<&Children>,
    mut scratch: Local<(Vec<Entity>, String)>,
    animal_need_points: Query<&CumulativeNeedPoints>,
    animal_diseases: Query<(), With<Disease>>,
    animal_pregnancies: Query<(), With<Pregnancy>>,
    guest_satisfaction: Query<&GuestSatisfaction>,
    staff_assignments: Query<&StaffAssignment>,
    facility_reservations: Query<&FacilityReservations>,
    facility_operating_dates: Query<&OperatingSinceDay>,
    facility_profits: Query<&FacilityProfit>,
    donation_totals: Query<&DonationTotal>,
    transport_vehicles: Query<&TransportVehicle>,
    zoo_clock: Res<ZooClock>,
    mut bound_values: Query<(&UiValueBinding, &mut UiValue, Option<&mut Text>)>,
    mut invalidation: InformationValueInvalidation,
) {
    let removed = &mut invalidation.removed;
    let dirty_subjects = invalidation
        .subjects
        .iter()
        .chain(removed.0.read())
        .chain(removed.1.read())
        .chain(removed.2.read())
        .chain(removed.3.read())
        .chain(removed.4.read())
        .chain(removed.5.read())
        .chain(removed.6.read())
        .chain(removed.7.read())
        .chain(removed.8.read())
        .chain(removed.9.read())
        .collect::<std::collections::HashSet<_>>();
    let mut dirty_rows = std::collections::HashSet::new();
    for node in invalidation
        .bindings
        .iter()
        .chain(invalidation.row_sources_removed.read())
        .chain(invalidation.rows_removed.read())
    {
        if let Some(row) = std::iter::once(node)
            .chain(invalidation.parents.iter_ancestors::<ChildOf>(node))
            .find(|&candidate| information_entity_sources.contains(candidate))
        {
            dirty_rows.insert(row);
        }
    }
    let day_changed = *invalidation.last_day != zoo_clock.absolute_day;
    *invalidation.last_day = zoo_clock.absolute_day;
    let (stack, formatted) = &mut *scratch;
    for (row, source) in &information_entity_sources {
        if !source.is_changed()
            && !dirty_subjects.contains(&source.0)
            && !dirty_rows.contains(&row)
            && !(day_changed && facility_operating_dates.contains(source.0))
        {
            continue;
        }
        visit_information_row(
            row,
            &children,
            &information_entity_sources,
            stack,
            |entity| {
                let Ok((value_binding, mut projected_value, text)) = bound_values.get_mut(entity)
                else {
                    return;
                };
                let Some((next_value, currency)) = project_information_list_value(
                    &value_binding.0,
                    source.0,
                    &animal_need_points,
                    &animal_diseases,
                    &animal_pregnancies,
                    &guest_satisfaction,
                    &staff_assignments,
                    &facility_reservations,
                    &facility_operating_dates,
                    &facility_profits,
                    &donation_totals,
                    &transport_vehicles,
                    zoo_clock.absolute_day,
                ) else {
                    return;
                };
                projected_value.set_if_neq(UiValue(next_value));
                if let Some(mut text) = text {
                    formatted.clear();
                    if currency {
                        write_whole_dollar_currency_with_thousands_separators(
                            formatted, next_value,
                        );
                    } else {
                        use std::fmt::Write;
                        let _ = write!(formatted, "{next_value}");
                    }
                    if text.0 != *formatted {
                        text.0.clone_from(formatted);
                    }
                }
            },
        );
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
    facility_reservations: &Query<&FacilityReservations>,
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
        UiIntegerPropertyBindingSource::FacilityCapacityUsed => facility_reservations
            .get(subject)
            .map(|reservations| i64::try_from(reservations.len()).unwrap_or(i64::MAX))
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
