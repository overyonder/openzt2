use std::fmt::Write;

use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId,
};

use crate::{
    assets::localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    plugins::ui::authored_ui_text_content_binding::UiTextBinding,
};

use super::donation_payment_types::DonationCategoryTotals;

/// Displays donation totals by category in Zoo Status.
pub(super) fn project_zoo_donation_category_totals_into_authored_summary(
    active_localization: Option<Res<LocalizationPrecedenceIndex>>,
    localizations: Res<Assets<LocalizationAsset>>,
    donation_acceptors: Query<&DonationCategoryTotals>,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    let localization = active_localization
        .as_deref()
        .and_then(|sources| sources.borrow_loaded_localization_view(&localizations));

    for (binding, mut text) in &mut fields {
        match &binding.0 {
            UiTextPropertyBindingSource::ZooDonationSummaryDonationCount { category, format } => {
                let Some((count, _)) =
                    aggregate_donation_totals_for_category(*category, donation_acceptors.iter())
                else {
                    continue;
                };
                text.0.clear();
                if localization.is_none_or(|localization| {
                    localization
                        .write_localized_text_with_format_arguments(
                            *format,
                            &[LocalizationFormatArgument::Integer(i64::from(count))],
                            &mut text.0,
                        )
                        .is_err()
                }) {
                    let _ = write!(text.0, "{count}");
                }
            }
            UiTextPropertyBindingSource::ZooDonationSummaryDonationAmount {
                category,
                positive_format,
            } => {
                let Some((_, amount_cents)) =
                    aggregate_donation_totals_for_category(*category, donation_acceptors.iter())
                else {
                    continue;
                };
                text.0.clear();
                let dollars = amount_cents / 100;
                let _ = write!(text.0, "${dollars}");
                if let Some(localization) = localization {
                    let unformatted_text = std::mem::take(&mut text.0);
                    if localization
                        .write_localized_text_with_format_arguments(
                            *positive_format,
                            &[LocalizationFormatArgument::Text(&unformatted_text)],
                            &mut text.0,
                        )
                        .is_err()
                    {
                        text.0 = unformatted_text;
                    }
                }
            }
            _ => {}
        }
    }
}

fn aggregate_donation_totals_for_category<'a>(
    requested_category: Option<AssetId>,
    donation_acceptors: impl Iterator<Item = &'a DonationCategoryTotals>,
) -> Option<(u32, i64)> {
    donation_acceptors
        .filter_map(|totals| totals.aggregate_for_category(requested_category))
        .try_fold((0_u32, 0_i64), |(count, amount), total| {
            Some((
                count.checked_add(total.count)?,
                amount.checked_add(total.amount.0)?,
            ))
        })
}
