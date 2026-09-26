use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::{
    InformationListCategory, InformationViewCategory,
};

use crate::plugins::{
    animal_lifecycle::types::{Animal, Pregnancy},
    animal_welfare::types::AnimalWelfare,
    donations::{
        donation_opportunity_types::DonationAcceptor, donation_payment_types::DonationTotal,
    },
    economy::facility_economy_types::{FacilityProfit, OperatingSinceDay, ServiceFacility},
    guests::guest_simulation_types::{Guest, GuestFavouriteAnimal, GuestSatisfaction},
    simulation_time::simulation_clock_types::ZooClock,
    staff::staff_employment_types::Staff,
    transport_tours::transport_vehicle_types::TransportVehicle,
    ui::authored_reusable_list_and_table_runtime_types::{SetUiListRowCount, UiListRow},
};

use super::super::{
    entity_selection_types::{InformationEntitySource, Inspectable},
    information_list_types::{
        InformationEntityListPanel, InformationEntityListSort,
        InformationListRowProjectionCandidate,
    },
    information_view_types::InformationViewClass,
};

/// Populates visible rows after sorting the eligible world entities.
pub(in crate::plugins::information) fn project_live_world_entities_to_visible_information_list_rows(
    information_list_panels: Query<(Entity, &InformationEntityListPanel, &InheritedVisibility)>,
    reusable_information_list_rows: Query<(Entity, &UiListRow, Option<&InformationEntitySource>)>,
    animals: Query<
        (
            Entity,
            &Inspectable,
            Option<&AnimalWelfare>,
            Option<&Pregnancy>,
        ),
        With<Animal>,
    >,
    guests: Query<
        (
            Entity,
            &Inspectable,
            Option<&GuestSatisfaction>,
            Option<&GuestFavouriteAnimal>,
        ),
        With<Guest>,
    >,
    staff: Query<(Entity, &Inspectable), With<Staff>>,
    buildings: Query<(
        Entity,
        &Inspectable,
        &InformationViewClass,
        Option<&ServiceFacility>,
        Option<&OperatingSinceDay>,
        Option<&FacilityProfit>,
    )>,
    donation_boxes: Query<
        (
            Entity,
            &Inspectable,
            Option<&DonationTotal>,
            Option<&OperatingSinceDay>,
            Option<&FacilityProfit>,
        ),
        With<DonationAcceptor>,
    >,
    vehicles: Query<(Entity, &Inspectable, &TransportVehicle)>,
    entity_names: Query<&Name>,
    zoo_clock: Res<ZooClock>,
    mut information_list_row_count_requests: MessageWriter<SetUiListRowCount>,
    mut commands: Commands,
) {
    for (panel_entity, information_list_panel, inherited_visibility) in &information_list_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let mut projection_candidates = Vec::new();
        let create_projection_candidate =
            |entity: Entity, inspectable: &Inspectable| InformationListRowProjectionCandidate {
                entity,
                definition: inspectable.definition.0,
                name: entity_names.get(entity).map_or("", Name::as_str),
                welfare: 0,
                pregnant: false,
                satisfaction: 0,
                favourite_animal: None,
                months_open: 0,
                profit_cents: 0,
                average_profit_cents: 0,
                current_capacity: 0,
                total_capacity: 0,
            };
        match information_list_panel.category {
            InformationListCategory::Animals => {
                animals
                    .iter()
                    .for_each(|(entity, inspectable, welfare, pregnancy)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
                        projection_candidate.welfare = welfare.map_or(0, |welfare| welfare.0);
                        projection_candidate.pregnant = pregnancy.is_some();
                        projection_candidates.push(projection_candidate);
                    })
            }
            InformationListCategory::Guests => {
                guests
                    .iter()
                    .for_each(|(entity, inspectable, satisfaction, favourite_animal)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
                        projection_candidate.satisfaction =
                            satisfaction.map_or(0, |satisfaction| satisfaction.0);
                        projection_candidate.favourite_animal = favourite_animal
                            .and_then(|favourite_animal| favourite_animal.0)
                            .map(|favourite_animal_identifier| favourite_animal_identifier.0);
                        projection_candidates.push(projection_candidate);
                    })
            }
            InformationListCategory::Staff => staff.iter().for_each(|(entity, inspectable)| {
                projection_candidates.push(create_projection_candidate(entity, inspectable));
            }),
            InformationListCategory::Buildings => buildings
                .iter()
                .filter(|(_, _, class, _, _, _)| class.0 == InformationViewCategory::Buildings)
                .for_each(|(entity, inspectable, _, facility, opened, profit)| {
                    let mut projection_candidate = create_projection_candidate(entity, inspectable);
                    if let Some(facility) = facility {
                        projection_candidate.current_capacity = facility.occupied;
                        projection_candidate.total_capacity = facility.capacity;
                    }
                    projection_candidate.months_open = opened.map_or(0, |opened| {
                        zoo_clock.absolute_day.saturating_sub(opened.0) / 30
                    });
                    if let Some(profit) = profit {
                        projection_candidate.profit_cents = profit.total.0;
                        projection_candidate.average_profit_cents = profit.average().0;
                    }
                    projection_candidates.push(projection_candidate);
                }),
            InformationListCategory::DonationBoxes => donation_boxes.iter().for_each(
                |(entity, inspectable, donation_total, opened, profit)| {
                    let mut projection_candidate = create_projection_candidate(entity, inspectable);
                    projection_candidate.months_open = opened.map_or(0, |opened| {
                        zoo_clock.absolute_day.saturating_sub(opened.0) / 30
                    });
                    if let Some(donation_total) = donation_total {
                        projection_candidate.profit_cents = donation_total.amount.0;
                        projection_candidate.average_profit_cents = (donation_total.count != 0)
                            .then(|| donation_total.amount.0 / i64::from(donation_total.count))
                            .unwrap_or(0);
                    }
                    if let Some(profit) = profit {
                        projection_candidate.profit_cents = profit.total.0;
                        projection_candidate.average_profit_cents = profit.average().0;
                    }
                    projection_candidates.push(projection_candidate);
                },
            ),
            InformationListCategory::Vehicles => {
                vehicles.iter().for_each(|(entity, inspectable, vehicle)| {
                    let mut projection_candidate = create_projection_candidate(entity, inspectable);
                    projection_candidate.current_capacity = vehicle.occupied;
                    projection_candidate.total_capacity = vehicle.seats;
                    projection_candidates.push(projection_candidate);
                })
            }
        }
        // Sort the bounded references used by this projection directly. This
        // is the sole multilist ordering point; no copied source container or
        // independently maintained sorting model survives the frame.
        projection_candidates.sort_unstable_by(|left_candidate, right_candidate| {
            compare_information_list_row_projection_candidates(
                left_candidate,
                right_candidate,
                information_list_panel.sort,
            )
        });
        information_list_row_count_requests.write(SetUiListRowCount {
            list: panel_entity,
            count: projection_candidates.len().min(usize::from(u16::MAX)) as u16,
        });
        for (row_entity, reusable_row, current_information_entity_source) in
            &reusable_information_list_rows
        {
            if reusable_row.list != panel_entity {
                continue;
            }
            if let Some(projection_candidate) =
                projection_candidates.get(usize::from(reusable_row.index))
            {
                if current_information_entity_source
                    .is_none_or(|current_source| current_source.0 != projection_candidate.entity)
                {
                    commands
                        .entity(row_entity)
                        .insert(InformationEntitySource(projection_candidate.entity));
                }
            } else if current_information_entity_source.is_some() {
                commands
                    .entity(row_entity)
                    .remove::<InformationEntitySource>();
            }
        }
    }
}

/// Compares rows using the requested sort field and direction.
fn compare_information_list_row_projection_candidates(
    left_candidate: &InformationListRowProjectionCandidate<'_>,
    right_candidate: &InformationListRowProjectionCandidate<'_>,
    requested_sort: InformationEntityListSort,
) -> std::cmp::Ordering {
    match requested_sort {
        InformationEntityListSort::SourceOrder => left_candidate
            .entity
            .to_bits()
            .cmp(&right_candidate.entity.to_bits()),
        InformationEntityListSort::Type { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate.definition.cmp(&right_candidate.definition),
                descending,
            )
            .then_with(|| {
                left_candidate
                    .entity
                    .to_bits()
                    .cmp(&right_candidate.entity.to_bits())
            })
        }
        InformationEntityListSort::Name { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate.name.cmp(right_candidate.name),
                descending,
            )
            .then_with(|| {
                left_candidate
                    .entity
                    .to_bits()
                    .cmp(&right_candidate.entity.to_bits())
            })
        }
        InformationEntityListSort::GuestFavouriteAnimal => left_candidate
            .favourite_animal
            .cmp(&right_candidate.favourite_animal)
            .then_with(|| {
                left_candidate
                    .entity
                    .to_bits()
                    .cmp(&right_candidate.entity.to_bits())
            }),
        InformationEntityListSort::Need { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate
                    .welfare
                    .max(left_candidate.satisfaction)
                    .cmp(&right_candidate.welfare.max(right_candidate.satisfaction)),
                descending,
            )
        }
        InformationEntityListSort::AnimalHappiness { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate.welfare.cmp(&right_candidate.welfare),
                descending,
            )
        }
        InformationEntityListSort::AnimalPregnancy { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate.pregnant.cmp(&right_candidate.pregnant),
                descending,
            )
        }
        InformationEntityListSort::MonthsOpen { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate.months_open.cmp(&right_candidate.months_open),
                descending,
            )
        }
        InformationEntityListSort::Profit { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate
                    .profit_cents
                    .cmp(&right_candidate.profit_cents),
                descending,
            )
        }
        InformationEntityListSort::AverageProfit { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate
                    .average_profit_cents
                    .cmp(&right_candidate.average_profit_cents),
                descending,
            )
        }
        InformationEntityListSort::CurrentCapacity { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate
                    .current_capacity
                    .cmp(&right_candidate.current_capacity),
                descending,
            )
        }
        InformationEntityListSort::TotalCapacity { descending } => {
            apply_requested_sort_direction_to_ordering(
                left_candidate
                    .total_capacity
                    .cmp(&right_candidate.total_capacity),
                descending,
            )
        }
    }
}

fn apply_requested_sort_direction_to_ordering(
    value: std::cmp::Ordering,
    descending: bool,
) -> std::cmp::Ordering {
    if descending {
        value.reverse()
    } else {
        value
    }
}
