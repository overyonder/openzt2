use super::original_saved_game_economy_conversion::money;
use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_zoo_state::economy::{
    general_economy_types::GeneralEconomyLedger,
    money_types::Money,
    monthly_finance_types::{MonthlyFinance, MonthlyFinanceHistory},
};
use std::{collections::BTreeMap, io};

pub(super) fn read_original_saved_general_economy(
    manager: &OrderedSourceDocumentNode,
    native_current_month: u32,
    calendar: ZooCalendar,
) -> io::Result<GeneralEconomyLedger> {
    let matching = manager
        .element_children()
        .filter(|node| {
            node.name == "ZTEconomyComponent"
                && node.attribute("category") == Some("generalEconomyComponent")
        })
        .collect::<Vec<_>>();
    let [component] = matching.as_slice() else {
        return Err(invalid(
            "saved economy requires one generalEconomyComponent",
        ));
    };
    if component.attribute("version") != Some("1") {
        return Err(invalid(
            "saved general economy requires version1 financial normalization",
        ));
    }
    let mut rows = BTreeMap::new();
    for row in component
        .element_children()
        .filter(|node| node.name == "BFGEconomyData")
    {
        let key = number::<u32>(row, "key")?;
        if rows.insert(key, row).is_some() {
            return Err(invalid("saved general economy repeats a month"));
        }
    }
    if rows.last_key_value().map(|(month, _)| *month) != Some(native_current_month) {
        return Err(invalid(
            "saved general history does not end at the current month",
        ));
    }
    let mut months = Vec::with_capacity(rows.len());
    for (month, row) in rows {
        let closing_cash = money(row, "cash")?;
        let income = money(row, "income")?;
        let expenses = money(row, "expenses")?;
        let opening_cash = if let Some(previous) = months
            .last()
            .map(|month: &MonthlyFinance| month.closing_cash)
        {
            previous
        } else {
            closing_cash
                .checked_sub(income)
                .and_then(|cash| cash.checked_add(expenses))
                .ok_or_else(|| invalid("saved general opening cash exceeds currency range"))?
        };
        months.push(MonthlyFinance {
            month_ordinal: month,
            opening_cash,
            closing_cash,
            income,
            expenses,
            ..Default::default()
        });
    }
    let mut history = MonthlyFinanceHistory::default();
    if !history.restore_saved_monthly_finance_history(
        &months,
        u32::from(calendar.year) * 12 + u32::from(calendar.month - 1),
        money(component, "lifetimeIncome")?,
        money(component, "lifetimeExpenses")?,
        Money::ZERO,
        0,
        !months.is_empty(),
    ) {
        return Err(invalid(
            "saved general economy violates canonical monthly history bounds",
        ));
    }
    GeneralEconomyLedger::from_saved(history)
        .ok_or_else(|| invalid("saved general economy is not aggregate-only"))
}
fn number<T: std::str::FromStr>(node: &OrderedSourceDocumentNode, name: &str) -> io::Result<T> {
    node.attribute(name)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| invalid(format!("{} has invalid {name}", node.name)))
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
