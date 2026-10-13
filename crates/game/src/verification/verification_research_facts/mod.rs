use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    economy::account_transaction_types::{Account, TransactionCompleted, TransactionKind},
    information::catalogue_types::SelectedCatalogueEntry,
    progression::{
        adoption_and_content_availability_types::ScenarioContentAvailability,
        catalogue_entry_availability::catalogue_entry_is_available,
        research_types::{PendingResearchProjectPayment, ResearchProject},
        unlock_types::UnlockedCatalogueDefinitionSet,
    },
    world_spawn::{
        selected_world_identity::SelectedWorldIdentity, world_membership_types::WorldRoot,
    },
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

#[derive(SystemParam)]
pub(super) struct VerificationResearchQueries<'w, 's> {
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    selected: Res<'w, SelectedCatalogueEntry>,
    unlocks: Res<'w, UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        'w,
        's,
        (
            &'static SelectedWorldIdentity,
            Option<&'static ScenarioContentAvailability>,
        ),
        With<WorldRoot>,
    >,
    projects: Query<'w, 's, &'static ResearchProject>,
    payments: Query<'w, 's, (), With<PendingResearchProjectPayment>>,
}

/// Research purchases and the selected catalogue item's research state.
pub(super) fn collect_research_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    world: VerificationResearchQueries,
    mut completed: MessageReader<TransactionCompleted>,
) {
    // Read every frame: settlement messages expire between journey snapshots.
    report.research_payment_cents.extend(
        completed
            .read()
            .filter(|payment| {
                payment.kind == TransactionKind::Research
                    && payment.debit == Account::Zoo
                    && payment.credit == Account::External
            })
            .map(|payment| payment.amount.0),
    );
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let definitions = world.active_definitions.get(&world.definitions);
    let entry = definitions
        .zip(world.selected.0)
        .and_then(|(definitions, selected)| {
            definitions
                .catalogue()
                .enumerate()
                .find(|(_, entry)| entry.definition == selected)
        });
    let research = definitions
        .zip(world.selected.0)
        .and_then(|(definitions, selected)| {
            definitions
                .research()
                .find(|research| research.id == selected || research.unlocks.contains(&selected))
        });
    let project = research.and_then(|research| {
        world
            .projects
            .iter()
            .find(|project| project.definition == research.id)
    });
    let world_identity = world.worlds.iter().next();
    let available = definitions
        .zip(entry)
        .is_some_and(|(definitions, (index, entry))| {
            catalogue_entry_is_available(
                definitions,
                index,
                entry,
                world_identity.map(|(identity, _)| identity.mode),
                world_identity.and_then(|(_, scenario)| scenario),
                &world.unlocks,
            )
        });
    let progress_basis_points = project.map_or(0, |project| {
        (u128::from(project.elapsed_ticks.min(project.required_ticks)) * 10_000)
            / u128::from(project.required_ticks.max(1))
    });
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    #[allow(clippy::cast_precision_loss)]
    let facts = [
        (
            "research_payments",
            count(report.research_payment_cents.len()),
        ),
        (
            "research_payment_cents",
            report.research_payment_cents.iter().sum::<i64>() as f64,
        ),
        ("research_projects", count(world.projects.iter().count())),
        (
            "research_pending_payments",
            count(world.payments.iter().count()),
        ),
        (
            "selected_catalogue_research_cost_cents",
            research.map_or(0.0, |research| research.cost_cents as f64),
        ),
        (
            "selected_catalogue_research_progress_basis_points",
            progress_basis_points as f64,
        ),
        (
            "selected_catalogue_entry_available",
            f64::from(u8::from(available)),
        ),
    ];
    for (name, value) in facts {
        report.current_facts.insert(name.to_owned(), value);
    }
}
