use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    document::UiDocumentRole, node_property_binding::UiIntegerPropertyBindingSource,
};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        donations::donation_payment_types::DonationTotal,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            authored_ui_node_projection_components::UiValue,
            authored_ui_node_projection_components::UiValueBinding,
        },
    },
};

use super::{
    show_schedule_types::{ScheduledShowRow, SelectedShowScheduleRow},
    show_stage_types::ShowStage,
};

/// Displays donations for the selected show, or all live stages when no row is selected.
pub(super) fn project_selected_or_all_show_donation_summary_into_authored_controls(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<(Entity, &UiDocumentRoot, Option<&ChildOf>)>,
    selections: Query<&SelectedShowScheduleRow>,
    scheduled: Query<&ScheduledShowRow>,
    totals: Query<&DonationTotal, With<ShowStage>>,
    mut values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
) {
    for (root, document_root, parent) in &roots {
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        if !matches!(
            &document.canonical_ui_document().role,
            UiDocumentRole::ShowEditor
        ) {
            continue;
        }
        let controller = parent.map_or(root, ChildOf::parent);
        let selected_stage = selections
            .get(controller)
            .ok()
            .and_then(|selection| scheduled.get(selection.0).ok())
            .map(|show| show.stage);
        let aggregate = selected_stage.map_or_else(
            || {
                totals
                    .iter()
                    .try_fold(DonationTotal::default(), |aggregate, total| {
                        Some(DonationTotal {
                            amount: crate::plugins::economy::money_types::Money(
                                aggregate.amount.0.checked_add(total.amount.0)?,
                            ),
                            count: aggregate.count.checked_add(total.count)?,
                        })
                    })
            },
            |stage| Some(totals.get(stage).copied().unwrap_or_default()),
        );
        let Some(aggregate) = aggregate else {
            continue;
        };
        for (owner, binding, mut value) in &mut values {
            if owner.0 != root {
                continue;
            }
            let next = match &binding.0 {
                UiIntegerPropertyBindingSource::DonationCount => Some(i64::from(aggregate.count)),
                UiIntegerPropertyBindingSource::DonationTotalCents => Some(aggregate.amount.0),
                UiIntegerPropertyBindingSource::DonationAverageCents => Some(
                    (aggregate.count != 0)
                        .then(|| aggregate.amount.0 / i64::from(aggregate.count))
                        .unwrap_or(0),
                ),
                _ => None,
            };
            if let Some(next) = next {
                value.0 = next;
            }
        }
    }
}
