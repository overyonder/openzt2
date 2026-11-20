use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
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
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::{
    donations::donation_payment_types::{DonationCategoryTotals, DonationTotal},
    economy::currency_formatting::write_whole_dollar_currency_with_thousands_separators,
    shows::show_stage_types::{ShowName, ShowStage},
    transport_tours::transport_circuit_types::TransportCircuit,
};

#[derive(Resource, Default, PartialEq)]
pub(super) struct FinanceDonationReports {
    species: Vec<DonationReportEntry>,
    tours: Vec<DonationReportEntry>,
    shows: Vec<DonationReportEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DonationReportSubject {
    Species(AssetId),
    Tour(Entity),
    Show(Entity),
}

#[derive(Component, PartialEq, Eq)]
pub(super) struct FinanceDonationRow(DonationReportSubject);

#[derive(PartialEq)]
struct DonationReportEntry {
    subject: DonationReportSubject,
    label: AssetId,
    name: String,
    total: DonationTotal,
}

pub(super) fn reduce_donations_for_finance_reports(
    mut report: ResMut<FinanceDonationReports>,
    species_index: Res<SpeciesAssets>,
    species_assets: Res<Assets<SpeciesAsset>>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    categories: Query<&DonationCategoryTotals>,
    tours: Query<(Entity, &TransportCircuit, Option<&Name>, &DonationTotal)>,
    shows: Query<
        (
            Entity,
            Option<&DefinitionId>,
            Option<&ShowName>,
            Option<&Name>,
            Option<&DonationTotal>,
        ),
        With<ShowStage>,
    >,
    changed: (
        Query<(), Changed<DonationCategoryTotals>>,
        Query<
            (),
            (
                With<TransportCircuit>,
                Or<(
                    Changed<Name>,
                    Changed<TransportCircuit>,
                    Changed<DonationTotal>,
                )>,
            ),
        >,
        Query<
            (),
            (
                With<ShowStage>,
                Or<(
                    Added<ShowStage>,
                    Changed<Name>,
                    Changed<ShowName>,
                    Changed<DonationTotal>,
                )>,
            ),
        >,
    ),
    mut removed: (
        RemovedComponents<DonationCategoryTotals>,
        RemovedComponents<DonationTotal>,
        RemovedComponents<ShowStage>,
        RemovedComponents<TransportCircuit>,
        RemovedComponents<Name>,
        RemovedComponents<ShowName>,
    ),
) {
    let removed_count = removed.0.read().count()
        + removed.1.read().count()
        + removed.2.read().count()
        + removed.3.read().count()
        + removed.4.read().count()
        + removed.5.read().count();
    if !report.is_added()
        && !species_index.is_changed()
        && !species_assets.is_changed()
        && !active_definitions.is_changed()
        && !definitions.is_changed()
        && removed_count == 0
        && changed.0.is_empty()
        && changed.1.is_empty()
        && changed.2.is_empty()
    {
        return;
    }
    let mut by_category = BTreeMap::<AssetId, DonationTotal>::new();
    for (category, amount) in categories.iter().flat_map(DonationCategoryTotals::iter) {
        let total = by_category.entry(category).or_default();
        total.count = total.count.saturating_add(amount.count);
        total.amount.0 = total.amount.0.saturating_add(amount.amount.0);
    }

    let mut next = FinanceDonationReports::default();
    if let Some(species) = species_index.get(&species_assets) {
        let mut emitted = BTreeSet::new();
        for record in species.species() {
            if !emitted.insert(record.id) {
                continue;
            }
            let keys = std::iter::once(record.authored_species_name.as_str())
                .chain(species.variants(record.id).filter_map(|variant| {
                    species
                        .find_variant_binding(variant.id)?
                        .authored_species_name_override
                        .as_deref()
                }))
                .map(|name| AssetId::from_key(&format!("Donate_{name}")))
                .collect::<BTreeSet<_>>();
            let mut total = DonationTotal::default();
            for key in keys {
                if let Some(category) = by_category.remove(&key) {
                    total.count = total.count.saturating_add(category.count);
                    total.amount.0 = total.amount.0.saturating_add(category.amount.0);
                }
            }
            if total.count != 0 {
                next.species.push(DonationReportEntry {
                    subject: DonationReportSubject::Species(record.id),
                    label: record.name_key,
                    name: record.authored_species_name.clone(),
                    total,
                });
            }
        }
    }
    let definitions = active_definitions.get(&definitions);
    for (tour, circuit, name, total) in &tours {
        let label = definitions
            .and_then(|definitions| definitions.find_object(circuit.definition))
            .map_or(AssetId::default(), |object| object.name_key);
        next.tours.push(DonationReportEntry {
            subject: DonationReportSubject::Tour(tour),
            label,
            name: name.map_or_else(String::new, |name| name.as_str().to_owned()),
            total: *total,
        });
    }
    next.tours
        .sort_unstable_by_key(|entry| match entry.subject {
            DonationReportSubject::Tour(entity) => entity.to_bits(),
            _ => 0,
        });
    for (show, definition, name, fallback, total) in &shows {
        let label = definition
            .and_then(|definition| definitions?.find_object(definition.0))
            .map_or(AssetId::default(), |object| object.name_key);
        next.shows.push(DonationReportEntry {
            subject: DonationReportSubject::Show(show),
            label,
            name: name
                .map(|name| name.0.as_str())
                .or_else(|| fallback.map(Name::as_str))
                .unwrap_or_default()
                .to_owned(),
            total: total.copied().unwrap_or_default(),
        });
    }
    next.shows
        .sort_unstable_by_key(|entry| match entry.subject {
            DonationReportSubject::Show(entity) => entity.to_bits(),
            _ => 0,
        });
    report.set_if_neq(next);
}

pub(super) fn project_finance_donation_rows(
    report: Res<FinanceDonationReports>,
    active_locale: Res<LocalizationPrecedenceIndex>,
    locales: Res<Assets<LocalizationAsset>>,
    mut lists: Query<(
        Ref<UiListPolicy>,
        Option<&Children>,
        &mut RequestedUiListRowCount,
    )>,
    mut rows: Query<(Ref<UiListRow>, Option<&mut FinanceDonationRow>)>,
    children: Query<&Children>,
    mut fields: Query<(&Name, &mut Text, Option<&mut UiValue>)>,
    mut commands: Commands,
) {
    let locale_changed = active_locale.is_changed() || locales.is_changed();
    let Some(locale) = active_locale.borrow_loaded_localization_view(&locales) else {
        return;
    };
    for (policy, owned, mut count) in &mut lists {
        use UiWidgetLiveCollectionSource::*;
        let report_rows = match policy.source {
            FinanceDonationsBySpecies => &report.species,
            FinanceTourDonations => &report.tours,
            FinanceShowDonations => &report.shows,
            _ => continue,
        };
        count.set_if_neq(RequestedUiListRowCount(Some(
            report_rows.len().min(usize::from(u16::MAX)) as u16,
        )));
        for entity in owned.into_iter().flat_map(RelationshipTarget::iter) {
            let Ok((row, previous)) = rows.get_mut(entity) else {
                continue;
            };
            let Some(entry) = report_rows.get(usize::from(row.index)) else {
                continue;
            };
            let mut changed =
                report.is_changed() || locale_changed || row.is_changed() || policy.is_changed();
            if let Some(mut previous) = previous {
                changed |= previous.set_if_neq(FinanceDonationRow(entry.subject));
            } else {
                commands
                    .entity(entity)
                    .insert(FinanceDonationRow(entry.subject));
                changed = true;
            }
            if !changed {
                continue;
            }
            let localized = locale
                .find_plain_localized_text(entry.label)
                .filter(|_| {
                    entry.name.is_empty()
                        || matches!(entry.subject, DonationReportSubject::Species(_))
                })
                .unwrap_or(entry.name.as_str());
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
                        let value = i64::from(entry.total.count);
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
                        let value = entry.total.amount.0;
                        if let Some(mut numeric) = numeric {
                            numeric.set_if_neq(UiValue(value));
                        }
                        write_whole_dollar_currency_with_thousands_separators(&mut rendered, value);
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

pub(super) fn clear_finance_donation_reports(mut reports: ResMut<FinanceDonationReports>) {
    reports.set_if_neq(FinanceDonationReports::default());
}
