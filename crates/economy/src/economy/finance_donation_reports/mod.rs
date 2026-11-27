use std::{collections::BTreeSet, fmt::Write};

use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, query::Allow,
        relationship::RelationshipTarget,
    },
    prelude::*,
};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::widget_live_collection::UiWidgetLiveCollectionSource, AssetId,
};
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        RequestedUiListRowCount, UiListPolicy, UiListRow,
    },
    authored_ui_node_projection_components::UiValue,
};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{DefinitionId, WorldMember, WorldMembers, WorldRoot},
};
use openzt2_zoo_state::{
    donations::donation_payment_types::{DonationTotal, ZooDonationSummary},
    shows::show_stage_types::{ShowName, ShowStage},
    transport_tours::transport_circuit_types::TransportCircuit,
};

#[derive(Component, Default, PartialEq)]
pub(super) struct FinanceDonationReports {
    species: Vec<SpeciesDonations>,
    tours: Vec<Entity>,
    shows: Vec<Entity>,
}

#[derive(PartialEq)]
struct SpeciesDonations {
    label: AssetId,
    total: DonationTotal,
}

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct DonationSpeciesCategories(Vec<(AssetId, BTreeSet<AssetId>)>);

pub(super) fn resolve_donation_species_categories(
    species_index: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    mut categories: ResMut<DonationSpeciesCategories>,
) {
    if !species_index.is_changed() && !species_assets.is_changed() {
        return;
    }
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    let mut emitted = BTreeSet::new();
    categories.set_if_neq(DonationSpeciesCategories(
        species
            .species()
            .filter(|record| emitted.insert(record.id))
            .map(|record| {
                let keys = std::iter::once(record.authored_species_name.as_str())
                    .chain(species.variants(record.id).filter_map(|variant| {
                        species
                            .find_variant_binding(variant.id)?
                            .authored_species_name_override
                            .as_deref()
                    }))
                    .map(|name| AssetId::from_key(&format!("Donate_{name}")))
                    .collect();
                (record.name_key, keys)
            })
            .collect(),
    ));
}

#[derive(Message)]
pub(super) struct RetiredFinanceDonationSubject(Entity);

pub(super) fn invalidate_retired_finance_donation_subjects(
    retired: On<Discard<(TransportCircuit, ShowStage, DonationTotal, WorldMember)>>,
    subjects: Query<
        &WorldMember,
        (
            Or<(With<TransportCircuit>, With<ShowStage>)>,
            Allow<Disabled>,
        ),
    >,
    mut invalidated: MessageWriter<RetiredFinanceDonationSubject>,
) {
    if let Ok(member) = subjects.get(retired.entity) {
        invalidated.write(RetiredFinanceDonationSubject(member.root));
    }
}

pub(super) fn reduce_donations_for_finance_reports(
    mut reports: Query<
        (
            Entity,
            Option<&WorldMembers>,
            Option<Ref<ZooDonationSummary>>,
            &mut FinanceDonationReports,
        ),
        With<WorldRoot>,
    >,
    species: Res<DonationSpeciesCategories>,
    tours: Query<(), (With<TransportCircuit>, With<DonationTotal>)>,
    stages: Query<(), With<ShowStage>>,
    changes: Query<
        &WorldMember,
        (
            Or<(With<TransportCircuit>, With<ShowStage>)>,
            Or<(
                Added<TransportCircuit>,
                Added<ShowStage>,
                Added<DonationTotal>,
                Changed<WorldMember>,
            )>,
        ),
    >,
    mut retired: MessageReader<RetiredFinanceDonationSubject>,
    mut removed_summaries: RemovedComponents<ZooDonationSummary>,
) {
    let affected: EntityHashSet = changes
        .iter()
        .map(|member| member.root)
        .chain(retired.read().map(|subject| subject.0))
        .collect();
    let removed_summaries: EntityHashSet = removed_summaries.read().collect();
    for (world, members, summary, mut report) in &mut reports {
        let lists_changed = report.is_added() || affected.contains(&world);
        let species_changed = report.is_added()
            || species.is_changed()
            || summary.as_ref().is_some_and(Ref::is_changed)
            || removed_summaries.contains(&world);
        let entities = || members.into_iter().flat_map(RelationshipTarget::iter);
        if species_changed {
            let mut seen = BTreeSet::new();
            let next = species
                .0
                .iter()
                .filter_map(|(label, keys)| {
                    let mut total = DonationTotal::default();
                    for &key in keys {
                        if seen.insert(key) {
                            if let Some(amount) = summary
                                .as_ref()
                                .and_then(|summary| summary.total(Some(key)))
                            {
                                add_total(&mut total, amount);
                            }
                        }
                    }
                    (total.count != 0).then_some(SpeciesDonations {
                        label: *label,
                        total,
                    })
                })
                .collect::<Vec<_>>();
            if report.species != next {
                report.species = next;
            }
        }
        if lists_changed {
            let mut tour_rows = entities()
                .filter(|&entity| tours.contains(entity))
                .collect::<Vec<_>>();
            let mut show_rows = entities()
                .filter(|&entity| stages.contains(entity))
                .collect::<Vec<_>>();
            tour_rows.sort_unstable_by_key(|entity| entity.to_bits());
            show_rows.sort_unstable_by_key(|entity| entity.to_bits());
            if report.tours != tour_rows {
                report.tours = tour_rows;
            }
            if report.shows != show_rows {
                report.shows = show_rows;
            }
        }
    }
}

fn add_total(total: &mut DonationTotal, amount: DonationTotal) {
    total.count = total.count.saturating_add(amount.count);
    total.amount = total.amount.saturating_add(amount.amount);
}

pub(super) fn project_finance_donation_rows(
    reports: Query<Ref<FinanceDonationReports>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    sources: Query<(
        Option<Ref<Name>>,
        Option<Ref<ShowName>>,
        Option<Ref<DonationTotal>>,
        Option<Ref<DefinitionId>>,
        Option<Ref<TransportCircuit>>,
    )>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut retired: (
        RemovedComponents<Name>,
        RemovedComponents<ShowName>,
        RemovedComponents<DonationTotal>,
        RemovedComponents<DefinitionId>,
    ),
    active_locale: Res<LocalizationPrecedenceIndex>,
    locales: Res<Assets<LocalizationAsset>>,
    mut lists: Query<(
        Ref<UiListPolicy>,
        Option<&Children>,
        &mut RequestedUiListRowCount,
    )>,
    rows: Query<Ref<UiListRow>>,
    children: Query<&Children>,
    mut fields: Query<(&Name, &mut Text, Option<&mut UiValue>)>,
) {
    let Ok(report) = reports.single() else {
        return;
    };
    let locale_changed = active_locale.is_changed() || locales.is_changed();
    let authors_changed = active_definitions.is_changed() || definitions.is_changed();
    let definitions = active_definitions.get(&definitions);
    let removed: EntityHashSet = retired
        .0
        .read()
        .chain(retired.1.read())
        .chain(retired.2.read())
        .chain(retired.3.read())
        .collect();
    let Some(locale) = active_locale.borrow_loaded_localization_view(&locales) else {
        return;
    };
    for (policy, owned, mut count) in &mut lists {
        use UiWidgetLiveCollectionSource::*;
        let (species, subjects): (&[SpeciesDonations], &[Entity]) = match policy.source {
            FinanceDonationsBySpecies => (&report.species, &[]),
            FinanceTourDonations => (&[], &report.tours),
            FinanceShowDonations => (&[], &report.shows),
            _ => continue,
        };
        count.set_if_neq(RequestedUiListRowCount(Some(
            (species.len() + subjects.len()).min(usize::from(u16::MAX)) as u16,
        )));
        for entity in owned.into_iter().flat_map(RelationshipTarget::iter) {
            let Ok(row) = rows.get(entity) else {
                continue;
            };
            let index = usize::from(row.index);
            let refresh = report.is_changed()
                || locale_changed
                || row.is_changed()
                || policy.is_changed()
                || authors_changed;
            let from_subject = subjects
                .get(index)
                .and_then(|&entity| sources.get(entity).ok().map(|facts| (entity, facts)));
            let (label, name, total) = if let Some(entry) = species.get(index) {
                if !refresh {
                    continue;
                }
                (Some(entry.label), None, entry.total)
            } else if let Some((entity, (name, show_name, total, id, circuit))) = from_subject {
                if !refresh
                    && !removed.contains(&entity)
                    && !name.as_ref().is_some_and(Ref::is_changed)
                    && !show_name.as_ref().is_some_and(Ref::is_changed)
                    && !total.as_ref().is_some_and(Ref::is_changed)
                    && !id.as_ref().is_some_and(Ref::is_changed)
                    && !circuit.as_ref().is_some_and(Ref::is_changed)
                {
                    continue;
                }
                let definition = circuit
                    .map(|circuit| circuit.definition)
                    .or_else(|| id.map(|id| id.0));
                let label = definition
                    .and_then(|id| definitions?.find_object(id))
                    .map(|object| object.name_key);
                let name = show_name
                    .map(|name| name.into_inner().0.as_str())
                    .or_else(|| name.map(|name| name.into_inner().as_str()));
                (
                    label,
                    name,
                    total.map_or(DonationTotal::default(), |total| *total),
                )
            } else {
                continue;
            };
            let localized = name
                .filter(|name| !name.is_empty())
                .or_else(|| label.and_then(|label| locale.find_plain_localized_text(label)))
                .unwrap_or_default();
            for node in std::iter::once(entity).chain(children.iter_descendants::<Children>(entity))
            {
                let Ok((name, mut text, numeric)) = fields.get_mut(node) else {
                    continue;
                };
                let mut rendered = String::new();
                match name.as_str().to_ascii_lowercase().as_str() {
                    "species" | "show name item" => {
                        rendered.push_str(localized);
                    }
                    "number" => {
                        let value = i64::from(total.count);
                        if let Some(mut numeric) = numeric {
                            numeric.set_if_neq(UiValue(value));
                        }
                        let format = AssetId::from_key("zoostatus:donations_list_num_color");
                        if locale
                            .write_localized_text_with_format_arguments(
                                format,
                                &[LocalizationFormatArgument::Integer(value)],
                                &mut rendered,
                            )
                            .is_err()
                        {
                            rendered.clear();
                            let _ = write!(rendered, "{value}");
                        }
                    }
                    "amount" => {
                        let value = super::display_currency_cents(total.amount);
                        if let Some(mut numeric) = numeric {
                            numeric.set_if_neq(UiValue(value));
                        }
                        let _ = locale.write_localized_currency_amount(value, false, &mut rendered);
                        let format = AssetId::from_key(if value < 0 {
                            "zoostatus:negformat"
                        } else {
                            "zoostatus:posformat"
                        });
                        let amount = std::mem::take(&mut rendered);
                        if locale
                            .write_localized_text_with_format_arguments(
                                format,
                                &[LocalizationFormatArgument::Text(&amount)],
                                &mut rendered,
                            )
                            .is_err()
                        {
                            rendered = amount;
                        }
                    }
                    _ => continue,
                }
                if text.0 != rendered {
                    text.0 = rendered;
                }
            }
        }
    }
}
