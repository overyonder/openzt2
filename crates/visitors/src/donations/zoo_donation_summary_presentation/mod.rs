use std::{collections::HashMap, fmt::Write};

use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId,
};

use openzt2_game_assets::localization::{
    localization_asset_types::LocalizationAsset,
    localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::donation_payment_types::{DonationCategoryTotals, DonationTotal};

#[derive(Resource, Default, PartialEq, Eq)]
pub(super) struct ZooDonationSummary(HashMap<Option<AssetId>, Option<DonationTotal>>);

pub(super) fn aggregate_zoo_donations_by_category(
    acceptors: Query<&DonationCategoryTotals>,
    changed_acceptors: Query<(), Changed<DonationCategoryTotals>>,
    mut removed_acceptors: RemovedComponents<DonationCategoryTotals>,
    mut summary: ResMut<ZooDonationSummary>,
) {
    let removed = removed_acceptors.read().count() != 0;
    if !removed && changed_acceptors.is_empty() {
        return;
    }
    let mut totals = HashMap::new();
    for (category, accepted) in acceptors.iter().flat_map(DonationCategoryTotals::iter) {
        for key in [Some(category), None] {
            let total = totals.entry(key).or_insert(Some(DonationTotal::default()));
            *total = total.and_then(|current| {
                Some(DonationTotal {
                    count: current.count.checked_add(accepted.count)?,
                    amount: openzt2_zoo_state::economy::money_types::Money(
                        current.amount.0.checked_add(accepted.amount.0)?,
                    ),
                })
            });
        }
    }
    summary.set_if_neq(ZooDonationSummary(totals));
}

pub(super) fn project_zoo_donation_summary(
    active_localization: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
    summary: Res<ZooDonationSummary>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let refresh_all = summary.is_changed()
        || localizations.is_changed()
        || active_localization
            .as_ref()
            .is_some_and(|index| index.is_changed());
    let localization = active_localization
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localizations));

    for (binding, mut text) in &mut fields {
        if !refresh_all && !binding.is_changed() && !text.is_added() {
            continue;
        }
        let category = match &binding.0 {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationCount { category, .. }
            | UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount { category, .. } => {
                *category
            }
            _ => continue,
        };
        let Some(total) = summary
            .0
            .get(&category)
            .copied()
            .unwrap_or(Some(DonationTotal::default()))
        else {
            continue;
        };
        let mut rendered = String::new();
        match &binding.0 {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationCount { format, .. } => {
                if localization.is_none_or(|localization| {
                    localization
                        .write_localized_text_with_format_arguments(
                            *format,
                            &[LocalizationFormatArgument::Integer(i64::from(total.count))],
                            &mut rendered,
                        )
                        .is_err()
                }) {
                    let _ = write!(rendered, "{}", total.count);
                }
            }
            UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount {
                positive_format,
                ..
            } => {
                openzt2_zoo_state::economy::currency_formatting::write_whole_dollar_currency_with_thousands_separators(
                    &mut rendered,
                    total.amount.0,
                );
                if let Some(localization) = localization {
                    let unformatted = std::mem::take(&mut rendered);
                    if localization
                        .write_localized_text_with_format_arguments(
                            *positive_format,
                            &[LocalizationFormatArgument::Text(&unformatted)],
                            &mut rendered,
                        )
                        .is_err()
                    {
                        rendered = unformatted;
                    }
                }
            }
            _ => unreachable!(),
        }
        text.map_unchanged(|text| &mut text.0).set_if_neq(rendered);
    }
}
