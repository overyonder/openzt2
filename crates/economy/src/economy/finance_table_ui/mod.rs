use super::{
    money_types::Money,
    monthly_finance_types::{MonthlyFinance, MonthlyFinanceHistory},
};
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::{
    localization::LocalizationFormatArgument,
    ui_document::{
        finance_table::{UiFinanceTableCategoryDefinition, UiFinanceTableValueSource},
        node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::RequestedUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiTablePolicy;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use std::fmt::Write;

use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::donations::donation_opportunity_types::DonationAcceptor;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;

#[derive(Default)]
pub(super) struct OrderedFinanceReportSubjects {
    buildings: Vec<Entity>,
    donation_boxes: Vec<Entity>,
}

pub(super) fn request_authored_finance_report_list_row_counts_and_bind_world_subjects(
    mut commands: Commands,
    history: Res<MonthlyFinanceHistory>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    owners: Query<&UiDocumentOwner>,
    parents: Query<&ChildOf>,
    lists: Query<(Entity, Ref<UiListPolicy>, Option<Ref<Children>>)>,
    mut counts: Query<&mut RequestedUiListRowCount>,
    mut rows: Query<(Ref<UiListRow>, Option<&mut InformationEntitySource>)>,
    buildings: Query<
        Entity,
        (
            With<Inspectable>,
            With<ServiceFacility>,
            Without<DonationAcceptor>,
        ),
    >,
    donation_boxes: Query<Entity, (With<Inspectable>, With<DonationAcceptor>)>,
    added: Query<
        (),
        Or<(
            Added<Inspectable>,
            Added<ServiceFacility>,
            Added<DonationAcceptor>,
        )>,
    >,
    mut removed: (
        RemovedComponents<Inspectable>,
        RemovedComponents<ServiceFacility>,
        RemovedComponents<DonationAcceptor>,
    ),
    mut subjects: Local<OrderedFinanceReportSubjects>,
) {
    let membership_changed = !added.is_empty()
        || !removed.0.is_empty()
        || !removed.1.is_empty()
        || !removed.2.is_empty();
    removed.0.clear();
    removed.1.clear();
    removed.2.clear();
    if membership_changed {
        subjects.buildings.clear();
        subjects.buildings.extend(buildings.iter());
        subjects
            .buildings
            .sort_unstable_by_key(|entity| entity.to_bits());
        subjects.donation_boxes.clear();
        subjects.donation_boxes.extend(donation_boxes.iter());
        subjects
            .donation_boxes
            .sort_unstable_by_key(|entity| entity.to_bits());
    }
    for (list, policy, children) in &lists {
        use UiWidgetLiveCollectionSource::*;
        let Some(subjects) = (match policy.source {
            FinanceBuildings => Some(&subjects.buildings),
            FinanceDonationBoxes => Some(&subjects.donation_boxes),
            _ => None,
        }) else {
            let count = match policy.source {
                FinanceBalanceSheetCategories | FinanceBalanceSheetMonthValues
                    if policy.is_changed() || documents.is_changed() =>
                {
                    finance_categories_for(list, &parents, &owners, &roots, &documents)
                        .map_or(0, <[_]>::len)
                }
                FinanceBalanceSheetMonths | FinanceBalanceSheetMonthColumns
                    if policy.is_changed() || history.is_changed() =>
                {
                    history.retained_month_finance_records().len()
                }
                _ => continue,
            };
            if let Ok(mut count_field) = counts.get_mut(list) {
                count_field.set_if_neq(RequestedUiListRowCount(Some(
                    count.min(usize::from(u16::MAX)) as u16,
                )));
            }
            continue;
        };
        if membership_changed || policy.is_changed() {
            if let Ok(mut count) = counts.get_mut(list) {
                count.set_if_neq(RequestedUiListRowCount(Some(
                    subjects.len().min(usize::from(u16::MAX)) as u16,
                )));
            }
        }
        if let Some(children) = children {
            for entity in RelationshipTarget::iter(&*children) {
                let Ok((row, source)) = rows.get_mut(entity) else {
                    continue;
                };
                if !membership_changed
                    && !policy.is_changed()
                    && !row.is_changed()
                    && source.is_some()
                {
                    continue;
                }
                match (subjects.get(usize::from(row.index)), source) {
                    (Some(&subject), Some(mut source)) => {
                        source.set_if_neq(InformationEntitySource(subject));
                    }
                    (Some(&subject), None) => {
                        commands
                            .entity(entity)
                            .insert(InformationEntitySource(subject));
                    }
                    (None, Some(_)) => {
                        commands.entity(entity).remove::<InformationEntitySource>();
                    }
                    (None, None) => {}
                }
            }
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FinanceBalanceSheetCell {
    list: Entity,
    category_index: Option<usize>,
    month_index: Option<usize>,
}

pub(super) fn bind_owning_balance_sheet_cells(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    rows: Query<&UiListRow>,
    lists: Query<&UiListPolicy>,
    fields: Query<(Entity, &UiTextBinding), (With<Text>, Without<FinanceBalanceSheetCell>)>,
) {
    use UiTextPropertyBindingSource::*;
    use UiWidgetLiveCollectionSource::*;

    for (entity, binding) in &fields {
        if !matches!(
            binding.0,
            FinanceBalanceSheetCategoryLabel
                | FinanceBalanceSheetCategoryValue
                | FinanceBalanceSheetMonthName
        ) {
            continue;
        }
        let mut category = None;
        let mut month = None;
        let mut current = entity;
        for parent in parents.iter_ancestors(entity) {
            if let (Ok(row), Ok(list)) = (rows.get(current), lists.get(parent)) {
                match list.source {
                    FinanceBalanceSheetCategories | FinanceBalanceSheetMonthValues => {
                        category.get_or_insert((parent, usize::from(row.index)));
                    }
                    FinanceBalanceSheetMonths | FinanceBalanceSheetMonthColumns => {
                        month.get_or_insert((parent, usize::from(row.index)));
                    }
                    _ => {}
                }
            }
            current = parent;
        }
        if let Some((list, _)) = category.or(month) {
            commands.entity(entity).insert(FinanceBalanceSheetCell {
                list,
                category_index: category.map(|(_, index)| index),
                month_index: month.map(|(_, index)| index),
            });
        }
    }
}

fn finance_categories_for<'a>(
    entity: Entity,
    parents: &Query<&ChildOf>,
    owners: &Query<&UiDocumentOwner>,
    roots: &Query<&UiDocumentRoot>,
    documents: &'a Assets<UiDocumentAsset>,
) -> Option<&'a [UiFinanceTableCategoryDefinition]> {
    std::iter::once(entity)
        .chain(parents.iter_ancestors(entity))
        .find_map(|entity| {
            let owner = owners.get(entity).ok()?;
            let root = roots.get(owner.0).ok()?;
            let document = documents.get(&root.document)?;
            document
                .canonical_ui_document()
                .nodes
                .iter()
                .find(|node| !node.finance_categories.is_empty())
                .map(|node| node.finance_categories.as_slice())
        })
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
    owners: Query<&UiDocumentOwner>,
    roots: Query<&UiDocumentRoot>,
    mut texts: Query<(Ref<FinanceBalanceSheetCell>, Ref<UiTextBinding>, &mut Text)>,
) {
    let refresh = history.is_changed()
        || active_localization.is_changed()
        || localizations.is_changed()
        || documents.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    let mut categories = std::collections::HashMap::new();
    for (cell, binding, mut text) in &mut texts {
        if !refresh && !cell.is_changed() && !binding.is_changed() && !text.is_added() {
            continue;
        }
        let categories = categories.entry(cell.list).or_insert_with(|| {
            finance_categories_for(cell.list, &parents, &owners, &roots, &documents)
        });
        let category = cell
            .category_index
            .and_then(|index| (*categories)?.get(index));
        let month = cell
            .month_index
            .and_then(|index| history.retained_month_finance_records().nth(index));
        let mut next = String::new();
        match &binding.0 {
            UiTextPropertyBindingSource::FinanceBalanceSheetCategoryLabel => {
                let Some(category) = category else {
                    continue;
                };
                next.push_str(
                    localization
                        .find_plain_localized_text(AssetId(category.label.0))
                        .unwrap_or(""),
                );
            }
            UiTextPropertyBindingSource::FinanceBalanceSheetCategoryValue => {
                let Some(category) = category else {
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

pub(super) fn request_authored_finance_table_row_counts(
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    lists: Query<(
        &UiDocumentOwner,
        Ref<UiTablePolicy>,
        &mut RequestedUiListRowCount,
    )>,
) {
    for (owner, policy, mut count) in lists {
        if !matches!(
            *policy,
            UiTablePolicy::FinanceLabels | UiTablePolicy::FinanceValues
        ) || (!policy.is_changed() && !documents.is_changed())
        {
            continue;
        }
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let rows = document
            .canonical_ui_document()
            .nodes
            .iter()
            .find(|node| !node.finance_categories.is_empty())
            .map(|node| node.finance_categories.len())
            .unwrap_or_default()
            .min(u16::MAX as usize) as u16;
        count.set_if_neq(RequestedUiListRowCount(Some(rows)));
    }
}

pub(super) fn project_monthly_finance_values_into_authored_table_rows(
    history: Res<MonthlyFinanceHistory>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    lists: Query<(&UiTablePolicy, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    rows: Query<(Entity, Ref<UiListRow>, Ref<ChildOf>)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    let localization_changed = active_localization.is_changed() || localizations.is_changed();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for (row_entity, row, parent) in &rows {
        let Ok((policy, owner)) = lists.get(parent.parent()) else {
            continue;
        };
        if !row.is_changed()
            && !parent.is_changed()
            && !history.is_changed()
            && !documents.is_changed()
            && !localization_changed
        {
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

use super::currency_formatting::write_whole_dollar_currency_with_thousands_separators;

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
