use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::ui_document::action::information::{
    InformationListCategory, InformationViewCategory,
};

use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::RequestedUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::Pregnancy;
use openzt2_zoo_state::animal_welfare::types::AnimalWelfare;
use openzt2_zoo_state::donations::donation_opportunity_types::DonationAcceptor;
use openzt2_zoo_state::donations::donation_payment_types::DonationTotal;
use openzt2_zoo_state::economy::facility_economy_types::FacilityProfit;
use openzt2_zoo_state::economy::facility_economy_types::OperatingSinceDay;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;
use openzt2_zoo_state::economy::service_types::FacilityReservations;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestFavouriteAnimal;
use openzt2_zoo_state::guests::guest_simulation_types::GuestSatisfaction;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::transport_tours::transport_rider_types::VehicleRiders;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::Inspectable;

use super::super::{
    information_list_types::{
        InformationEntityListPanel, InformationEntityListSort, InformationListOrder,
        InformationListRowProjectionCandidate,
    },
    information_view_types::InformationViewClass,
};

use super::information_list_membership::ManagementListMembers;

#[derive(SystemParam)]
pub(crate) struct InformationListSortInputs<'w, 's> {
    definition: Query<'w, 's, (), Changed<Inspectable>>,
    names: Query<'w, 's, (), (With<Inspectable>, Changed<Name>)>,
    happiness: Query<
        'w,
        's,
        (),
        (
            With<Inspectable>,
            Or<(Changed<AnimalWelfare>, Changed<GuestSatisfaction>)>,
        ),
    >,
    pregnancy: Query<'w, 's, (), (With<Inspectable>, Added<Pregnancy>)>,
    favourites: Query<'w, 's, (), (With<Inspectable>, Changed<GuestFavouriteAnimal>)>,
    opened: Query<'w, 's, (), (With<Inspectable>, Changed<OperatingSinceDay>)>,
    profit: Query<
        'w,
        's,
        (),
        (
            With<Inspectable>,
            Or<(Changed<FacilityProfit>, Changed<DonationTotal>)>,
        ),
    >,
    capacity: Query<
        'w,
        's,
        (),
        (
            With<Inspectable>,
            Or<(
                Changed<ServiceFacility>,
                Changed<FacilityReservations>,
                Changed<TransportVehicle>,
                Changed<VehicleRiders>,
            )>,
        ),
    >,
    removed: (
        RemovedComponents<'w, 's, Name>,
        RemovedComponents<'w, 's, Pregnancy>,
        RemovedComponents<'w, 's, FacilityReservations>,
        RemovedComponents<'w, 's, AnimalWelfare>,
        RemovedComponents<'w, 's, GuestSatisfaction>,
        RemovedComponents<'w, 's, GuestFavouriteAnimal>,
        RemovedComponents<'w, 's, FacilityProfit>,
        RemovedComponents<'w, 's, DonationTotal>,
        RemovedComponents<'w, 's, OperatingSinceDay>,
        RemovedComponents<'w, 's, ServiceFacility>,
        RemovedComponents<'w, 's, VehicleRiders>,
    ),
}

type Subjects = bevy::ecs::entity::EntityHashSet;

struct RemovedSortInputs {
    names: Subjects,
    pregnancies: Subjects,
    capacity: Subjects,
    happiness: Subjects,
    favourites: Subjects,
    profit: Subjects,
    opened: Subjects,
}

impl InformationListSortInputs<'_, '_> {
    fn take_removed_inputs(&mut self) -> RemovedSortInputs {
        let (
            names,
            pregnancies,
            reservations,
            welfare,
            satisfaction,
            favourites,
            profit,
            totals,
            opened,
            facilities,
            riders,
        ) = &mut self.removed;
        RemovedSortInputs {
            names: names.read().collect(),
            pregnancies: pregnancies.read().collect(),
            capacity: reservations
                .read()
                .chain(facilities.read())
                .chain(riders.read())
                .collect(),
            happiness: welfare.read().chain(satisfaction.read()).collect(),
            favourites: favourites.read().collect(),
            profit: profit.read().chain(totals.read()).collect(),
            opened: opened.read().collect(),
        }
    }

    fn active_sort_key_changed(
        &self,
        sort: InformationEntityListSort,
        members: &[Entity],
        removed: &RemovedSortInputs,
        day_changed: bool,
    ) -> bool {
        use InformationEntityListSort::*;
        let contains_removed = |removed: &Subjects| {
            !removed.is_empty() && members.iter().any(|member| removed.contains(member))
        };
        match sort {
            SourceOrder => false,
            Type { .. } => {
                !self.definition.is_empty()
                    && members
                        .iter()
                        .any(|&member| self.definition.contains(member))
            }
            Name { .. } => {
                contains_removed(&removed.names)
                    || (!self.names.is_empty()
                        && members.iter().any(|&member| self.names.contains(member)))
            }
            Need { .. } | AnimalHappiness { .. } => {
                contains_removed(&removed.happiness)
                    || (!self.happiness.is_empty()
                        && members
                            .iter()
                            .any(|&member| self.happiness.contains(member)))
            }
            AnimalPregnancy { .. } => {
                contains_removed(&removed.pregnancies)
                    || (!self.pregnancy.is_empty()
                        && members
                            .iter()
                            .any(|&member| self.pregnancy.contains(member)))
            }
            GuestFavouriteAnimal => {
                contains_removed(&removed.favourites)
                    || (!self.favourites.is_empty()
                        && members
                            .iter()
                            .any(|&member| self.favourites.contains(member)))
            }
            MonthsOpen { .. } => {
                day_changed
                    || contains_removed(&removed.opened)
                    || (!self.opened.is_empty()
                        && members.iter().any(|&member| self.opened.contains(member)))
            }
            Profit { .. } | AverageProfit { .. } => {
                contains_removed(&removed.profit)
                    || (!self.profit.is_empty()
                        && members.iter().any(|&member| self.profit.contains(member)))
            }
            CurrentCapacity { .. } | TotalCapacity { .. } => {
                contains_removed(&removed.capacity)
                    || (!self.capacity.is_empty()
                        && members.iter().any(|&member| self.capacity.contains(member)))
            }
        }
    }
}

/// Populates visible rows after sorting the eligible world entities.
pub(crate) fn project_live_world_entities_to_visible_information_list_rows(
    mut information_list_panels: Query<(
        Entity,
        Ref<InformationEntityListPanel>,
        Ref<InheritedVisibility>,
        Option<Ref<Children>>,
        &mut InformationListOrder,
        &mut RequestedUiListRowCount,
    )>,
    mut reusable_information_list_rows: Query<(
        Ref<UiListRow>,
        Option<&mut InformationEntitySource>,
    )>,
    mut membership: ManagementListMembers,
    changed_rows: Query<&ChildOf, Changed<UiListRow>>,
    mut inputs: InformationListSortInputs,
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
        Option<&FacilityReservations>,
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
    vehicles: Query<(
        Entity,
        &Inspectable,
        &TransportVehicle,
        Option<&VehicleRiders>,
    )>,
    entity_names: Query<&Name>,
    zoo_clock: Res<ZooClock>,
    mut previous_day: Local<u32>,
    mut commands: Commands,
) {
    let removed = membership.removals();
    let removed_sort_inputs = inputs.take_removed_inputs();
    let day_changed = *previous_day != zoo_clock.absolute_day;
    *previous_day = zoo_clock.absolute_day;
    let dirty_lists = changed_rows
        .iter()
        .map(ChildOf::parent)
        .collect::<std::collections::HashSet<_>>();
    for (list, information_list_panel, visibility, children, mut order, mut requested_count) in
        &mut information_list_panels
    {
        let (members, group_changed) = membership.members(information_list_panel.category, removed);
        let rebuild_order = information_list_panel.is_changed()
            || (visibility.is_changed() && visibility.get())
            || group_changed
            || inputs.active_sort_key_changed(
                information_list_panel.sort,
                members,
                &removed_sort_inputs,
                day_changed,
            );
        if !visibility.get() {
            continue;
        }
        if rebuild_order {
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
                InformationListCategory::Animals => animals
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .for_each(|(entity, inspectable, welfare, pregnancy)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
                        projection_candidate.welfare = welfare.map_or(0, |welfare| welfare.0);
                        projection_candidate.pregnant = pregnancy.is_some();
                        projection_candidates.push(projection_candidate);
                    }),
                InformationListCategory::Guests => guests
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .for_each(|(entity, inspectable, satisfaction, favourite_animal)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
                        projection_candidate.satisfaction =
                            satisfaction.map_or(0, |satisfaction| satisfaction.0);
                        projection_candidate.favourite_animal = favourite_animal
                            .and_then(|favourite_animal| favourite_animal.0)
                            .map(|favourite_animal_identifier| favourite_animal_identifier.0);
                        projection_candidates.push(projection_candidate);
                    }),
                InformationListCategory::Staff => staff
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .for_each(|(entity, inspectable)| {
                        projection_candidates
                            .push(create_projection_candidate(entity, inspectable));
                    }),
                InformationListCategory::Buildings => buildings
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .filter(|(_, _, class, _, _, _, _)| {
                        class.0 == InformationViewCategory::Buildings
                    })
                    .for_each(
                        |(entity, inspectable, _, facility, reservations, opened, profit)| {
                            let mut projection_candidate =
                                create_projection_candidate(entity, inspectable);
                            if let Some(facility) = facility {
                                projection_candidate.current_capacity =
                                    u16::try_from(reservations.map_or(0, RelationshipTarget::len))
                                        .unwrap_or(u16::MAX);
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
                        },
                    ),
                InformationListCategory::DonationBoxes => donation_boxes
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .for_each(|(entity, inspectable, donation_total, opened, profit)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
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
                    }),
                InformationListCategory::Vehicles => vehicles
                    .iter_many(members.iter().copied())
                    .filter_map(|row| row.ok())
                    .for_each(|(entity, inspectable, vehicle, riders)| {
                        let mut projection_candidate =
                            create_projection_candidate(entity, inspectable);
                        projection_candidate.current_capacity = riders
                            .map_or(0, |riders| u16::try_from(riders.len()).unwrap_or(u16::MAX));
                        projection_candidate.total_capacity = vehicle.seats;
                        projection_candidates.push(projection_candidate);
                    }),
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
            order.0.clear();
            order.0.extend(
                projection_candidates
                    .iter()
                    .map(|candidate| candidate.entity),
            );
            requested_count.set_if_neq(RequestedUiListRowCount(Some(
                order.0.len().min(usize::from(u16::MAX)) as u16,
            )));
        }
        if rebuild_order
            || dirty_lists.contains(&list)
            || children.as_ref().is_some_and(Ref::is_changed)
        {
            bind_information_list_rows(
                children.as_deref(),
                &order.0,
                &mut reusable_information_list_rows,
                &mut commands,
            );
        }
    }
}

pub(super) fn bind_information_list_rows(
    children: Option<&Children>,
    subjects: &[Entity],
    rows: &mut Query<(Ref<UiListRow>, Option<&mut InformationEntitySource>)>,
    commands: &mut Commands,
) {
    for row_entity in children.into_iter().flat_map(RelationshipTarget::iter) {
        let Ok((row, source)) = rows.get_mut(row_entity) else {
            continue;
        };
        match (subjects.get(usize::from(row.index)), source) {
            (Some(&subject), Some(mut source)) => {
                source.set_if_neq(InformationEntitySource(subject));
            }
            (Some(&subject), None) => {
                commands
                    .entity(row_entity)
                    .insert(InformationEntitySource(subject));
            }
            (None, Some(_)) => {
                commands
                    .entity(row_entity)
                    .remove::<InformationEntitySource>();
            }
            (None, None) => {}
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
