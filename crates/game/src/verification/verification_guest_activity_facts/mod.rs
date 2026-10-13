use bevy::prelude::*;
use openzt2_game_data::behavior::eligibility::BehaviorCandidateRole;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestNeedKind;

use crate::plugins::{
    animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy,
    behavior_task_execution_types::BehaviorTaskExecutionState,
    economy::account_transaction_types::{Account, TransactionCompleted, TransactionKind},
    guests::guest_simulation_types::{
        Guest, GuestArchetype, GuestGift, GuestHunger, GuestPhase, Viewing,
    },
    topology::topology_graph_types::PathTile,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_game_assets::{
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Guest task starts count every task a guest began during the journey.
/// Guests admitted and inside the zoo are visiting. Guests watching an animal, those watching it from a path, and those whose
/// task targets an object through a type family rather than its own name,
/// such as a seating task on `bench` taken to a `bench_df`. Purchases are
/// guest payments to the zoo. Repeats are later payments by the same guest to
/// the same stand, which a single purchase never makes. Held items are objects
/// in a guest's hands or containers, such as a bought hamburger. Guests who
/// began eating ran a task on a held item, such as that hamburger. Fed guests
/// are those whose hunger wellness has risen since such a task began.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
pub(super) fn collect_guest_activity_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    viewers: Query<&Viewing, With<Guest>>,
    tasks: Query<Ref<BehaviorTaskExecutionState>, With<Guest>>,
    paths: Query<(), With<PathTile>>,
    targets: Query<&DefinitionId>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut completed: MessageReader<TransactionCompleted>,
    guests: Query<(), With<Guest>>,
    items: Query<Entity, With<DefinitionId>>,
    occupancy: Res<InteractionContainerOccupancy>,
    eaters: Query<(Entity, &BehaviorTaskExecutionState, &GuestHunger), With<Guest>>,
    hunger: Query<&GuestHunger>,
    gifts: Query<(&GuestArchetype, &GuestGift, &GuestPhase), With<Guest>>,
) {
    // Read every frame: settlement messages expire between journey snapshots.
    for payment in completed.read() {
        if payment.credit != Account::Zoo
            || !matches!(payment.debit, Account::Entity(payer) if guests.contains(payer))
        {
            continue;
        }
        report.guest_income_cents += payment.amount.0;
        if payment.kind == TransactionKind::Service {
            report.guest_purchases.push(*payment);
        }
    }
    // Count task starts every frame: a refused task can begin and end
    // between journey snapshots.
    report.guest_task_starts = report
        .guest_task_starts
        .saturating_add(tasks.iter().filter(|task| task.is_added()).count() as u32);
    // Track hunger every frame: an eating task can begin and end between
    // journey snapshots.
    for (guest, task, guest_hunger) in &eaters {
        if task.object.is_some() {
            report
                .guest_hunger_when_eating_began
                .entry(guest)
                .or_insert(guest_hunger.value);
        }
    }
    let fed = report
        .guest_hunger_when_eating_began
        .iter()
        .filter(|(guest, began)| hunger.get(**guest).is_ok_and(|now| now.value > **began))
        .map(|(guest, _)| *guest)
        .collect::<Vec<_>>();
    report.guests_fed_by_eating.extend(fed);
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    let from_paths = viewers
        .iter()
        .filter(|viewing| viewing.area.is_some_and(|area| paths.contains(area)))
        .count();
    let definitions = world_definitions.get(&world_assets);
    let at_type_families = tasks
        .iter()
        .filter(|task| {
            // Nested sets keep the target contract of the task that chose it.
            let (document, declaration) = task
                .stack
                .first()
                .map_or((&task.document, task.declaration), |root| {
                    (&root.document, root.declaration)
                });
            let Some(object) = task
                .target
                .and_then(|target| targets.get(target).ok())
                .and_then(|definition| definitions?.find_object(definition.0))
            else {
                return false;
            };
            documents
                .get(document)
                .and_then(|asset| asset.behavior_task_at_index(declaration))
                .is_some_and(|authored| {
                    let mut named = authored
                        .candidate_eligibility_requirements
                        .iter()
                        .filter(|requirement| {
                            requirement.candidate_role == BehaviorCandidateRole::Target
                        })
                        .flat_map(|requirement| &requirement.candidate_type_identifiers);
                    !named.clone().any(|&required| required == object.id)
                        && named.any(|required| object.behavior_type_identifiers.contains(required))
                })
        })
        .count();
    // Gift wellness at or below the authored trigger, as the need triggers use.
    let wanting_gifts = gifts
        .iter()
        .filter(|(archetype, gift, _)| {
            definitions
                .and_then(|definitions| definitions.find_guest(archetype.0))
                .and_then(|guest| {
                    guest
                        .needs
                        .iter()
                        .find(|need| need.kind == GuestNeedKind::Gift)
                })
                .is_some_and(|need| {
                    gift.value <= 1_000_u16.saturating_sub(need.reconsider_threshold)
                })
        })
        .count();
    let held_items = items
        .iter()
        .filter(|item| {
            occupancy
                .container_for_member(*item)
                .is_some_and(|(holder, _)| guests.contains(holder))
        })
        .count();
    for (name, value) in [
        ("guests_viewing", count(viewers.iter().count())),
        (
            "guests_visiting",
            count(
                gifts
                    .iter()
                    .filter(|(_, _, phase)| **phase == GuestPhase::Visiting)
                    .count(),
            ),
        ),
        ("guests_viewing_from_paths", count(from_paths)),
        ("guests_tasked_at_type_families", count(at_type_families)),
        ("guests_wanting_gifts", count(wanting_gifts)),
        (
            "cash_cents_without_guest_income",
            report
                .current_facts
                .get("cash_cents")
                .copied()
                .unwrap_or_default()
                - report.guest_income_cents as f64,
        ),
        ("guest_purchases", count(report.guest_purchases.len())),
        (
            "guest_purchase_cents",
            report
                .guest_purchases
                .iter()
                .map(|payment| payment.amount.0 as f64)
                .sum(),
        ),
        ("guest_held_items", count(held_items)),
        ("guest_task_starts", f64::from(report.guest_task_starts)),
        (
            "guests_began_eating",
            count(report.guest_hunger_when_eating_began.len()),
        ),
        (
            "guests_fed_by_eating",
            count(report.guests_fed_by_eating.len()),
        ),
        (
            "guest_repeat_purchases",
            count(
                report
                    .guest_purchases
                    .iter()
                    .enumerate()
                    .filter(|(index, payment)| {
                        report.guest_purchases[..*index].iter().any(|earlier| {
                            earlier.debit == payment.debit && earlier.subject == payment.subject
                        })
                    })
                    .count(),
            ),
        ),
    ] {
        report.current_facts.insert(name.to_owned(), value);
    }
}
