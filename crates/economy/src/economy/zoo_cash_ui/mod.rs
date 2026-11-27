use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    money_types::Money,
    zoo_cash_types::{UnlimitedZooCash, ZooCash},
};
use bevy::prelude::*;
use openzt2_game_assets::{
    localization::loaded_localization_queries::CurrencyLocalization,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::{
        action::economy::UiEconomyAction, node_property_binding::UiTextPropertyBindingSource,
    },
    AssetId,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiEconomyActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiCashGrantPendingTransaction;

/// Projects the authoritative zoo account balance into authored cash fields.
/// The String owned by Bevy's Text component is retained across updates.
pub(super) fn project_zoo_cash_balance_into_authored_ui_text(
    worlds: Query<(Ref<ZooCash>, Ref<UnlimitedZooCash>), With<WorldRoot>>,
    currency: CurrencyLocalization,
    mut fields: Query<(Ref<UiTextBinding>, &mut Text, &mut TextColor, &mut TextFont)>,
) {
    let Ok((cash, unlimited)) = worlds.single() else {
        return;
    };
    let is_unlimited = unlimited.0;
    let policy_changed = unlimited.is_changed();
    let localization_changed = currency.is_changed();
    let localization = currency.loaded_text();

    for (binding, mut text, mut color, mut font) in &mut fields {
        let UiTextPropertyBindingSource::ZooCash {
            positive_format,
            negative_format,
        } = &binding.0
        else {
            continue;
        };
        if !cash.is_changed() && !policy_changed && !localization_changed && !binding.is_added() {
            continue;
        }

        text.0.clear();
        let selected_format;
        if is_unlimited {
            if let Some(value) = localization.and_then(|localization| {
                localization.find_plain_localized_text(AssetId::from_key("economy:unlimitedcash"))
            }) {
                text.0.push_str(value);
            }
            selected_format = *positive_format;
        } else {
            let dollars = cash.0.dollars().trunc() as i64;
            let _ = currency.write_amount(dollars.saturating_mul(100), false, &mut text.0);
            selected_format = if dollars < 0 {
                *negative_format
            } else {
                *positive_format
            };
        }

        if let Some(localization) = localization {
            let unformatted_text = std::mem::take(&mut text.0);
            if localization
                .write_localized_text_with_format_arguments(
                    selected_format,
                    &[LocalizationFormatArgument::Text(&unformatted_text)],
                    &mut text.0,
                )
                .is_err()
            {
                text.0 = unformatted_text;
            }
            apply_localized_cash_text_presentation(
                localization,
                selected_format,
                &mut color,
                &mut font,
            );
        }
    }
}

fn apply_localized_cash_text_presentation(
    localization: openzt2_game_assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    format_key: AssetId,
    color: &mut TextColor,
    font: &mut TextFont,
) {
    let Some(presentation) = localization.find_localized_text_presentation(format_key) else {
        return;
    };
    if let Some(color_rgba) = presentation.text_color_rgba {
        color.0 = Color::srgba_u8(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]);
    }
    if let Some(font_size) = presentation.font_size_pixels {
        font.font_size = FontSize::Px(font_size);
    }
    if presentation.bold_text {
        font.weight = FontWeight::BOLD;
    }
}

pub(super) fn request_cash_grant_transactions_from_authored_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiEconomyActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    worlds: Query<Entity, With<WorldRoot>>,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    let Ok(root) = worlds.single() else {
        return;
    };
    for activation in activations.read() {
        let Ok((action_records, document_owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(document_root) = roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };

        for record in action_records.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            let UiEconomyAction::GrantZooCash { amount } = &record.action else {
                continue;
            };
            let amount = Money::from_cents(*amount);
            if amount.is_positive() {
                let operation = commands.spawn((
                    UiCashGrantPendingTransaction,
                    WorldMember { root },
                    openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction::CashGrant,
                )).id();
                transactions.write(TransactionRequest {
                    domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
                    operation,
                    debit: Account::External,
                    credit: Account::Zoo,
                    amount,
                    kind: TransactionKind::Reward,
                    subject: None,
                });
            }
        }
    }
}

pub(super) fn remove_resolved_ui_cash_grant_transaction_entities(
    mut commands: Commands,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(), With<UiCashGrantPendingTransaction>>,
) {
    for operation in completed
        .read()
        .map(|result| result.operation)
        .chain(rejected.read().map(|result| result.operation))
    {
        if pending.get(operation).is_ok() {
            commands.entity(operation).despawn();
        }
    }
}
