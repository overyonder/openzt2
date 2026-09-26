use super::{
    money_types::Money,
    monthly_finance_types::{MonthlyFinance, MonthlyFinanceHistory},
};
use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    },
    plugins::ui::{
        authored_reusable_list_and_table_runtime_types::{
            SetUiListRowCount, UiListPolicy, UiListRow, UiTablePolicy,
        },
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_text_content_binding::UiTextBinding,
    },
};
use bevy::prelude::*;
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::{
        finance_table::UiFinanceTableValueSource,
        node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};
use std::fmt::Write;

use crate::plugins::{
    donations::donation_opportunity_types::DonationAcceptor,
    economy::facility_economy_types::ServiceFacility,
    information::entity_selection_types::{InformationEntitySource, Inspectable},
};

/// Sizes the native-created management report lists and binds building rows
/// directly to their canonical world entities. The report component in the
/// original executable created these rows imperatively; their Bevy equivalent
/// remains an ordinary typed live collection.
pub(super) fn request_authored_finance_report_list_row_counts_and_bind_world_subjects(
    mut commands: Commands,
    history: Res<MonthlyFinanceHistory>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner)>,
    rows: Query<(Entity, &UiListRow, Option<&InformationEntitySource>)>,
    buildings: Query<
        Entity,
        (
            With<Inspectable>,
            With<ServiceFacility>,
            Without<DonationAcceptor>,
        ),
    >,
    donation_boxes: Query<Entity, (With<Inspectable>, With<DonationAcceptor>)>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    for (list_entity, policy, owner) in &lists {
        let category_count = || {
            roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
                .and_then(|document| {
                    document
                        .canonical_ui_document()
                        .nodes
                        .iter()
                        .find(|node| !node.finance_categories.is_empty())
                })
                .map_or(0, |node| node.finance_categories.len())
        };
        let row_subjects = match policy.source {
            UiWidgetLiveCollectionSource::FinanceBuildings => {
                Some(buildings.iter().collect::<Vec<_>>())
            }
            UiWidgetLiveCollectionSource::FinanceDonationBoxes => {
                Some(donation_boxes.iter().collect::<Vec<_>>())
            }
            _ => None,
        };
        let count = match policy.source {
            UiWidgetLiveCollectionSource::FinanceBalanceSheetCategories
            | UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthValues => category_count(),
            UiWidgetLiveCollectionSource::FinanceBalanceSheetMonths
            | UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthColumns => {
                history.retained_month_finance_records().len()
            }
            UiWidgetLiveCollectionSource::FinanceBuildings
            | UiWidgetLiveCollectionSource::FinanceDonationBoxes => {
                row_subjects.as_ref().map_or(0, Vec::len)
            }
            UiWidgetLiveCollectionSource::FinanceDonationsBySpecies
            | UiWidgetLiveCollectionSource::FinanceTourDonations
            | UiWidgetLiveCollectionSource::FinanceShowDonations => 0,
            _ => continue,
        };
        row_count_requests.write(SetUiListRowCount {
            list: list_entity,
            count: count.min(usize::from(u16::MAX)) as u16,
        });
        let Some(mut row_subjects) = row_subjects else {
            continue;
        };
        row_subjects.sort_unstable_by_key(|entity| entity.to_bits());
        for (row_entity, row, current_source) in &rows {
            if row.list != list_entity {
                continue;
            }
            if let Some(subject) = row_subjects.get(usize::from(row.index)).copied() {
                if current_source.is_none_or(|current| current.0 != subject) {
                    commands
                        .entity(row_entity)
                        .insert(InformationEntitySource(subject));
                }
            } else if current_source.is_some() {
                commands
                    .entity(row_entity)
                    .remove::<InformationEntitySource>();
            }
        }
    }
}

/// Projects category labels, month headings, and per-month category values
/// into the nested native balance-sheet lists created from shipped fragments.
#[allow(clippy::too_many_arguments)]
pub(super) fn project_monthly_finance_values_into_native_balance_sheet_lists(
    history: Res<MonthlyFinanceHistory>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    parents: Query<&ChildOf>,
    rows: Query<&UiListRow>,
    lists: Query<&UiListPolicy>,
    mut texts: Query<(Entity, &UiTextBinding, &mut Text)>,
) {
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (text_entity, binding, mut text) in &mut texts {
        if !matches!(
            &binding.0,
            UiTextPropertyBindingSource::FinanceBalanceSheetCategoryLabel
                | UiTextPropertyBindingSource::FinanceBalanceSheetCategoryValue
                | UiTextPropertyBindingSource::FinanceBalanceSheetMonthName
        ) {
            continue;
        }
        let Some(categories) = documents.iter().find_map(|(_, document)| {
            document
                .canonical_ui_document()
                .nodes
                .iter()
                .find(|node| !node.finance_categories.is_empty())
                .map(|node| node.finance_categories.as_slice())
        }) else {
            continue;
        };
        let row_contexts = find_ui_list_rows_in_ancestors(text_entity, &rows, &lists, &parents);
        let category_index = row_contexts.iter().find_map(|(row, source)| {
            matches!(
                source,
                UiWidgetLiveCollectionSource::FinanceBalanceSheetCategories
                    | UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthValues
            )
            .then_some(usize::from(row.index))
        });
        let month_index = row_contexts.iter().find_map(|(row, source)| {
            matches!(
                source,
                UiWidgetLiveCollectionSource::FinanceBalanceSheetMonths
                    | UiWidgetLiveCollectionSource::FinanceBalanceSheetMonthColumns
            )
            .then_some(usize::from(row.index))
        });
        let month =
            month_index.and_then(|index| history.retained_month_finance_records().nth(index));
        let mut next = String::new();
        match &binding.0 {
            UiTextPropertyBindingSource::FinanceBalanceSheetCategoryLabel => {
                let Some(category) = category_index.and_then(|index| categories.get(index)) else {
                    continue;
                };
                next.push_str(
                    localization
                        .find_plain_localized_text(AssetId(category.label.0))
                        .unwrap_or(""),
                );
            }
            UiTextPropertyBindingSource::FinanceBalanceSheetCategoryValue => {
                let Some(category) = category_index.and_then(|index| categories.get(index)) else {
                    continue;
                };
                let (amount, currency) =
                    select_monthly_finance_category_value(month, &category.value_source);
                if currency {
                    write_whole_dollar_currency_with_thousands_separators(&mut next, amount);
                } else {
                    let _ = write!(next, "{amount}");
                }
            }
            UiTextPropertyBindingSource::FinanceBalanceSheetMonthName => {
                let Some(month) = month else {
                    continue;
                };
                next.push_str(
                    localization
                        .localized_month_abbreviation(
                            u8::try_from(month.month_ordinal % 12 + 1).unwrap_or(1),
                        )
                        .unwrap_or(""),
                );
            }
            _ => continue,
        }
        if text.0 != next {
            text.0 = next;
        }
    }
}

fn find_ui_list_rows_in_ancestors<'a>(
    descendant: Entity,
    rows: &'a Query<&UiListRow>,
    lists: &Query<&UiListPolicy>,
    parents: &Query<&ChildOf>,
) -> Vec<(&'a UiListRow, UiWidgetLiveCollectionSource)> {
    let mut output = Vec::with_capacity(2);
    let mut candidate = descendant;
    for _ in 0..64 {
        if let Ok(row) = rows.get(candidate) {
            if let Ok(policy) = lists.get(row.list) {
                output.push((row, policy.source));
            }
        }
        let Ok(parent) = parents.get(candidate) else {
            break;
        };
        candidate = parent.parent();
    }
    output
}

pub(super) fn request_authored_finance_table_row_counts(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    lists: Query<(Entity, &UiDocumentOwner, Ref<UiTablePolicy>)>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    for (list, owner, policy) in &lists {
        if !matches!(
            *policy,
            UiTablePolicy::FinanceLabels | UiTablePolicy::FinanceValues
        ) || (!policy.is_added() && !policy.is_changed() && !documents.is_changed())
        {
            continue;
        }
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        row_count_requests.write(SetUiListRowCount {
            list,
            count: document
                .canonical_ui_document()
                .nodes
                .iter()
                .find(|node| !node.finance_categories.is_empty())
                .map(|node| node.finance_categories.len())
                .unwrap_or_default()
                .min(u16::MAX as usize) as u16,
        });
    }
}

pub(super) fn project_monthly_finance_values_into_authored_table_rows(
    history: Res<MonthlyFinanceHistory>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    lists: Query<(&UiTablePolicy, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    rows: Query<(Entity, Ref<UiListRow>)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    let localization_changed = active_localization.is_changed() || localizations.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (row_entity, row) in &rows {
        let Ok((policy, owner)) = lists.get(row.list) else {
            continue;
        };
        if !row.is_added() && !row.is_changed() && !history.is_changed() && !localization_changed {
            continue;
        }
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let Some(category) = document
            .canonical_ui_document()
            .nodes
            .iter()
            .find(|node| !node.finance_categories.is_empty())
            .and_then(|node| node.finance_categories.get(usize::from(row.index)))
        else {
            continue;
        };

        let mut projected_text = String::new();
        match policy {
            UiTablePolicy::FinanceLabels => {
                projected_text.push_str(
                    localization
                        .find_plain_localized_text(AssetId(category.label.0))
                        .unwrap_or(""),
                );
            }
            UiTablePolicy::FinanceValues => {
                let (amount, is_currency) = select_monthly_finance_category_value(
                    history.current_month_finance(),
                    &category.value_source,
                );
                if is_currency {
                    let formatted = (category.format.0 != [0; 16])
                        .then(|| {
                            localization.write_localized_text_with_format_arguments(
                                AssetId(category.format.0),
                                &[LocalizationFormatArgument::CurrencyCents(amount)],
                                &mut projected_text,
                            )
                        })
                        .is_some_and(|result| result.is_ok());
                    if !formatted {
                        write_whole_dollar_currency_with_thousands_separators(
                            &mut projected_text,
                            amount,
                        );
                    }
                } else {
                    let _ = write!(projected_text, "{amount}");
                }
            }
            _ => continue,
        }
        replace_text_on_entity_and_descendants(row_entity, &projected_text, &children, &mut texts);
    }
}

fn select_monthly_finance_category_value(
    month: Option<&MonthlyFinance>,
    source: &UiFinanceTableValueSource,
) -> (i64, bool) {
    let Some(month) = month else {
        return (
            0,
            !matches!(source, UiFinanceTableValueSource::AdmissionsCount),
        );
    };
    let value = match source {
        UiFinanceTableValueSource::AdmissionsCount => return (i64::from(month.total_users), false),
        UiFinanceTableValueSource::AdmissionIncome => month.admission_income,
        UiFinanceTableValueSource::CashGrants => Money(
            month.cash_grants.0.saturating_add(
                (month.month_ordinal == 0)
                    .then_some(month.opening_cash.0)
                    .unwrap_or(0),
            ),
        ),
        UiFinanceTableValueSource::DonationIncome => month.donation_income,
        UiFinanceTableValueSource::FoodDrinkSales => month.food_drink_sales,
        UiFinanceTableValueSource::RecyclingIncome => month.recycling_income,
        UiFinanceTableValueSource::GiftSales => month.gift_sales,
        UiFinanceTableValueSource::AnimalAdoption => month.animal_adoption,
        UiFinanceTableValueSource::AnimalUpkeep => month.animal_upkeep,
        UiFinanceTableValueSource::Construction => month.construction,
        UiFinanceTableValueSource::Research => month.research,
        UiFinanceTableValueSource::StaffSalaries => month.staff_salaries,
        UiFinanceTableValueSource::Upkeep => month.upkeep,
        UiFinanceTableValueSource::ClosingCash => month.closing_cash,
    };
    // Authored report categories request profit, while the accounting owner
    // deliberately stores expense magnitudes as nonnegative amounts.
    let expense = matches!(
        source,
        UiFinanceTableValueSource::AnimalAdoption
            | UiFinanceTableValueSource::AnimalUpkeep
            | UiFinanceTableValueSource::Construction
            | UiFinanceTableValueSource::Research
            | UiFinanceTableValueSource::StaffSalaries
            | UiFinanceTableValueSource::Upkeep
    );
    (
        if expense {
            value.0.saturating_neg()
        } else {
            value.0
        },
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_profit_negates_expense_magnitudes_without_mutating_accounts() {
        let month = MonthlyFinance {
            animal_adoption: Money(100_000),
            staff_salaries: Money(70_000),
            construction: Money(12_000),
            admission_income: Money(4_000),
            ..Default::default()
        };
        for (source, expected) in [
            (UiFinanceTableValueSource::AnimalAdoption, -100_000),
            (UiFinanceTableValueSource::StaffSalaries, -70_000),
            (UiFinanceTableValueSource::Construction, -12_000),
            (UiFinanceTableValueSource::AdmissionIncome, 4_000),
        ] {
            assert_eq!(
                select_monthly_finance_category_value(Some(&month), &source),
                (expected, true)
            );
        }
        assert_eq!(month.animal_adoption, Money(100_000));
    }
}

pub(crate) fn write_whole_dollar_currency_with_thousands_separators(
    output: &mut String,
    cents: i64,
) {
    // Reports truncate cents before choosing the sign, so -99 cents displays as $0.
    let whole_dollar_cents = cents / 100 * 100;
    let _ = openzt2_game_data::localization::LocalizationCatalog::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
        "$",
        whole_dollar_cents,
        false,
        output,
    );
}

fn replace_text_on_entity_and_descendants(
    entity: Entity,
    value: &str,
    children: &Query<&Children>,
    texts: &mut Query<&mut Text>,
) {
    if let Ok(mut text) = texts.get_mut(entity) {
        if text.0 != value {
            text.0.clear();
            text.0.push_str(value);
        }
    }
    let Ok(entity_children) = children.get(entity) else {
        return;
    };
    for child in entity_children.iter() {
        replace_text_on_entity_and_descendants(child, value, children, texts);
    }
}
