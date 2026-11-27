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
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use std::fmt::Write;

use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::donations::donation_opportunity_types::DonationAcceptor;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;

#[derive(Component)]
#[relationship(relationship_target = FinanceReportRows)]
pub(super) struct FinanceReportSubject(Entity);

#[derive(Component)]
#[relationship_target(relationship = FinanceReportSubject)]
pub(super) struct FinanceReportRows(Vec<Entity>);

pub(super) fn bind_finance_report_rows(
    mut commands: Commands,
    lists: Query<(Entity, Ref<UiListPolicy>, Option<&Children>)>,
    mut counts: Query<&mut RequestedUiListRowCount>,
    rows: Query<(Entity, Ref<UiListRow>, Option<&FinanceReportSubject>)>,
    subjects: Query<
        (Entity, Has<DonationAcceptor>),
        (
            With<Inspectable>,
            Or<(With<ServiceFacility>, With<DonationAcceptor>)>,
        ),
    >,
    changed_subjects: Query<
        (),
        (
            With<Inspectable>,
            Or<(
                Added<Inspectable>,
                Added<ServiceFacility>,
                Added<DonationAcceptor>,
            )>,
        ),
    >,
    mut removed: (
        RemovedComponents<Inspectable>,
        RemovedComponents<ServiceFacility>,
        RemovedComponents<DonationAcceptor>,
    ),
) {
    let removed_count =
        removed.0.read().count() + removed.1.read().count() + removed.2.read().count();
    let membership_changed = removed_count != 0 || !changed_subjects.is_empty();
    let mut ordered_subjects = None;
    for (list, policy, children) in &lists {
        let donation_box = match policy.source {
            UiWidgetLiveCollectionSource::FinanceBuildings => false,
            UiWidgetLiveCollectionSource::FinanceDonationBoxes => true,
            _ => continue,
        };
        if !membership_changed
            && !policy.is_changed()
            && children.is_none_or(|children| {
                rows.iter_many(children.iter())
                    .flatten()
                    .all(|(_, row, subject)| !row.is_changed() && subject.is_some())
            })
        {
            continue;
        }
        let ordered = ordered_subjects.get_or_insert_with(|| {
            let mut ordered = [Vec::new(), Vec::new()];
            for (subject, acceptor) in &subjects {
                ordered[usize::from(acceptor)].push(subject);
            }
            for subjects in &mut ordered {
                subjects.sort_unstable_by_key(|subject: &Entity| subject.to_bits());
            }
            ordered
        });
        let subjects = &ordered[usize::from(donation_box)];
        if let Ok(mut count) = counts.get_mut(list) {
            count.set_if_neq(RequestedUiListRowCount(Some(
                subjects.len().min(usize::from(u16::MAX)) as u16,
            )));
        }
        if let Some(children) = children {
            for (entity, row, current) in rows.iter_many(children.iter()).flatten() {
                let subject = subjects.get(usize::from(row.index)).copied();
                if subject == current.map(|current| current.0) {
                    continue;
                }
                if let Some(subject) = subject {
                    commands
                        .entity(entity)
                        .insert(FinanceReportSubject(subject));
                } else {
                    commands.entity(entity).remove::<FinanceReportSubject>();
                }
            }
        }
    }
}

pub(super) fn project_finance_report_subjects(
    mut commands: Commands,
    mut rows: Query<
        (
            Entity,
            &FinanceReportSubject,
            Option<&mut InformationEntitySource>,
        ),
        Changed<FinanceReportSubject>,
    >,
    linked_rows: Query<(), With<FinanceReportSubject>>,
    mut removed: RemovedComponents<FinanceReportSubject>,
) {
    for entity in removed
        .read()
        .filter(|&entity| !linked_rows.contains(entity))
    {
        commands
            .entity(entity)
            .try_remove::<InformationEntitySource>();
    }
    for (entity, subject, source) in &mut rows {
        let next = InformationEntitySource(subject.0);
        if let Some(mut source) = source {
            source.set_if_neq(next);
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

pub(super) fn request_finance_balance_sheet_list_sizes(
    worlds: Query<Ref<MonthlyFinanceHistory>, With<WorldRoot>>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    owners: Query<&UiDocumentOwner>,
    parents: Query<&ChildOf>,
    mut lists: Query<(Entity, Ref<UiListPolicy>, &mut RequestedUiListRowCount)>,
) {
    let Ok(history) = worlds.single() else {
        return;
    };
    use UiWidgetLiveCollectionSource::*;
    for (list, policy, mut count) in &mut lists {
        let rows = match policy.source {
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
        count.set_if_neq(RequestedUiListRowCount(Some(
            rows.min(usize::from(u16::MAX)) as u16,
        )));
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
    worlds: Query<Ref<MonthlyFinanceHistory>, With<WorldRoot>>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    parents: Query<&ChildOf>,
    owners: Query<&UiDocumentOwner>,
    roots: Query<&UiDocumentRoot>,
    mut texts: Query<(Ref<FinanceBalanceSheetCell>, Ref<UiTextBinding>, &mut Text)>,
) {
    let Ok(history) = worlds.single() else {
        return;
    };
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
                    let _ = localization.write_localized_currency_amount(amount, false, &mut next);
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
    worlds: Query<Ref<MonthlyFinanceHistory>, With<WorldRoot>>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    lists: Query<(&UiTablePolicy, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    rows: Query<(Entity, Ref<UiListRow>, Ref<ChildOf>)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    let Ok(history) = worlds.single() else {
        return;
    };
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
                        let _ = localization.write_localized_currency_amount(
                            amount,
                            false,
                            &mut projected_text,
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
        UiFinanceTableValueSource::CashGrants => month.cash_grants.saturating_add(
            (month.month_ordinal == 0)
                .then_some(month.opening_cash)
                .unwrap_or(Money::ZERO),
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
            super::display_currency_cents(-value)
        } else {
            super::display_currency_cents(value)
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
            animal_adoption: Money::from_cents(100_000),
            staff_salaries: Money::from_cents(70_000),
            construction: Money::from_cents(12_000),
            admission_income: Money::from_cents(4_000),
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
        assert_eq!(month.animal_adoption, Money::from_cents(100_000));
    }
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
