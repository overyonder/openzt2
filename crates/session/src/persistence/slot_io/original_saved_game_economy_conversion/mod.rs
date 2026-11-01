use std::{collections::BTreeMap, io};

use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_zoo_state::economy::{
    authored_transaction_amount::authored_currency_to_money,
    money_types::Money,
    monthly_finance_types::{MonthlyFinance, MonthlyFinanceHistory},
};

use super::economy_snapshot_types::EconomySnapshotRecords;

pub(super) fn read_original_saved_economy(
    manager: &OrderedSourceDocumentNode,
    status: &OrderedSourceDocumentNode,
    native_current_month: u32,
    calendar: ZooCalendar,
    unlimited_cash: bool,
) -> io::Result<EconomySnapshotRecords> {
    let mut categories = BTreeMap::new();
    for component in manager
        .element_children()
        .filter(|node| node.name.as_str() == "ZTEconomyComponent")
    {
        let category = required(component, "category")?;
        if categories.insert(category, component).is_some() {
            return Err(invalid(format!(
                "duplicate saved economy category {category}"
            )));
        }
    }
    let zoo = *categories
        .get("Zoo")
        .ok_or_else(|| invalid("saved economy has no Zoo category"))?;
    let general = *categories
        .get("generalEconomyComponent")
        .ok_or_else(|| invalid("saved economy has no general economy component"))?;
    let mut zoo_months = BTreeMap::new();
    for row in zoo
        .element_children()
        .filter(|node| node.name.as_str() == "BFGEconomyData")
    {
        let month = read_u32(row, "key")?;
        if zoo_months.insert(month, row).is_some() {
            return Err(invalid(format!("duplicate saved zoo month {month}")));
        }
    }
    let category_amount = |category: &str, month: u32, field: &str| -> io::Result<Money> {
        categories
            .get(category)
            .map(|component| month_row(component, month))
            .transpose()?
            .flatten()
            .map(|row| money(row, field))
            .transpose()
            .map(|amount| amount.unwrap_or(Money::ZERO))
    };
    let mut months = Vec::with_capacity(zoo_months.len());
    for (&month, row) in &zoo_months {
        let closing_cash = money(row, "cash")?;
        let income = money(row, "income")?;
        let expenses = money(row, "expenses")?;
        let opening_cash = match months.last().map(|row: &MonthlyFinance| row.closing_cash) {
            Some(previous) => previous,
            None => Money(
                closing_cash
                    .0
                    .checked_sub(income.0)
                    .and_then(|cash| cash.checked_add(expenses.0))
                    .ok_or_else(|| invalid("saved opening cash exceeds the money range"))?,
            ),
        };
        let general_month = month_row(general, month)?;
        months.push(MonthlyFinance {
            month_ordinal: month,
            opening_cash,
            closing_cash,
            income,
            expenses,
            admission_income: category_amount("admissions_income", month, "income")?,
            cash_grants: category_amount("cash_grants", month, "income")?,
            donation_income: category_amount("donations_income", month, "income")?,
            food_drink_sales: category_amount("concessions_food_drink", month, "income")?,
            recycling_income: category_amount("recycling", month, "income")?,
            gift_sales: category_amount("concessions_gifts", month, "income")?,
            animal_adoption: category_amount("animal", month, "expenses")?,
            animal_upkeep: category_amount("animal_upkeep", month, "expenses")?,
            construction: category_amount("construction", month, "expenses")?,
            research: category_amount("research", month, "expenses")?,
            staff_salaries: category_amount("staff_salaries", month, "expenses")?,
            upkeep: category_amount("upkeep", month, "expenses")?,
            active_users: general_month
                .map(|row| read_i32(row, "users"))
                .transpose()?
                .unwrap_or(0),
            total_users: general_month
                .map(|row| read_i32(row, "totalUsers"))
                .transpose()?
                .unwrap_or(0),
        });
    }
    if zoo_months.last_key_value().map(|(month, _)| *month) != Some(native_current_month) {
        return Err(invalid(
            "saved monthly history does not end at the current month",
        ));
    }
    let current = zoo_months.get(&native_current_month).ok_or_else(|| {
        invalid(format!(
            "saved zoo has no current month {native_current_month}"
        ))
    })?;
    let lifetime_users = required(general, "lifetimeUsers")?
        .parse::<i64>()
        .map_err(|_| invalid("saved lifetime guest count is not i64"))?;
    let economy = EconomySnapshotRecords {
        zoo_cash: money(current, "cash")?,
        unlimited_cash: Some(unlimited_cash),
        admission_price: money(status, "adultAdmission")?,
        admissions_open: match required(status, "zoo_is_open")? {
            "true" => true,
            "false" => false,
            _ => return Err(invalid("saved zoo open state is not a boolean")),
        },
        current_calendar_month: u32::from(calendar.year) * 12
            + u32::from(calendar.month.saturating_sub(1)),
        lifetime_income: money(zoo, "lifetimeIncome")?,
        lifetime_expenses: money(zoo, "lifetimeExpenses")?,
        lifetime_donation_income: categories
            .get("donations_income")
            .map(|component| money(component, "lifetimeIncome"))
            .transpose()?
            .unwrap_or(Money::ZERO),
        lifetime_users,
        monthly_finance_history_initialized: !months.is_empty(),
        monthly_finance_records: months,
        entity_economy_records: Vec::new(),
    };
    let mut validation = MonthlyFinanceHistory::default();
    if !validation.restore_saved_monthly_finance_history(
        &economy.monthly_finance_records,
        economy.current_calendar_month,
        economy.lifetime_income,
        economy.lifetime_expenses,
        economy.lifetime_donation_income,
        economy.lifetime_users,
        economy.monthly_finance_history_initialized,
    ) {
        return Err(invalid(
            "saved zoo finances cannot be represented by the monthly history",
        ));
    }
    Ok(economy)
}

fn month_row(
    component: &OrderedSourceDocumentNode,
    month: u32,
) -> io::Result<Option<&OrderedSourceDocumentNode>> {
    let mut row = None;
    for candidate in component
        .element_children()
        .filter(|node| node.name.as_str() == "BFGEconomyData")
    {
        if read_u32(candidate, "key")? == month && row.replace(candidate).is_some() {
            return Err(invalid(format!("duplicate saved component month {month}")));
        }
    }
    Ok(row)
}

pub(super) fn money(node: &OrderedSourceDocumentNode, field: &str) -> io::Result<Money> {
    let amount = required(node, field)?
        .parse::<f64>()
        .map_err(|_| invalid(format!("saved {field} is not a currency amount")))?;
    let magnitude = authored_currency_to_money(amount.abs())
        .ok_or_else(|| invalid(format!("saved {field} exceeds the money range")))?;
    Ok(if amount.is_sign_negative() {
        Money(-magnitude.0)
    } else {
        magnitude
    })
}

fn read_u32(node: &OrderedSourceDocumentNode, field: &str) -> io::Result<u32> {
    required(node, field)?
        .parse()
        .map_err(|_| invalid(format!("saved {field} is not u32")))
}
fn read_i32(node: &OrderedSourceDocumentNode, field: &str) -> io::Result<i32> {
    required(node, field)?
        .parse()
        .map_err(|_| invalid(format!("saved {field} is not i32")))
}
fn required<'a>(node: &'a OrderedSourceDocumentNode, field: &str) -> io::Result<&'a str> {
    node.attribute(field)
        .ok_or_else(|| invalid(format!("saved {} has no {field}", node.name)))
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn economy_uses_current_zoo_cash_and_preserves_carried_visitors() -> io::Result<()> {
        let document = parse_blue_fang_source_document(AssetPath::new("saved/world.zt2"),br#"
          <ZTEconomyMgr>
            <ZTEconomyComponent category="generalEconomyComponent" lifetimeUsers="5">
              <BFGEconomyData key="0" cash="7000" income="7000" expenses="0" users="4" totalUsers="4"/>
              <BFGEconomyData key="1" cash="8000" income="1000" expenses="0" users="3" totalUsers="1"/>
            </ZTEconomyComponent>
            <ZTEconomyComponent category="Zoo" lifetimeIncome="1250" lifetimeExpenses="300">
              <BFGEconomyData key="0" cash="1000" income="1200" expenses="200"/>
              <BFGEconomyData key="1" cash="950" income="50" expenses="100"/>
            </ZTEconomyComponent>
            <ZTEconomyComponent category="admissions_income"><BFGEconomyData key="1" income="50"/></ZTEconomyComponent>
            <ZTEconomyComponent category="construction"><BFGEconomyData key="0" expenses="200"/><BFGEconomyData key="1" expenses="100"/></ZTEconomyComponent>
          </ZTEconomyMgr>"#).map_err(io::Error::other)?;
        let status = parse_blue_fang_source_document(
            AssetPath::new("saved/world.zt2"),
            br#"<ZTStatus adultAdmission="20" zoo_is_open="false"/>"#,
        )
        .map_err(io::Error::other)?;
        let economy = read_original_saved_economy(
            &document.root,
            &status.root,
            1,
            ZooCalendar {
                year: 1,
                month: 2,
                day: 1,
            },
            true,
        )?;
        assert_eq!(economy.zoo_cash, Money(95_000));
        assert_eq!(economy.admission_price, Money(2_000));
        assert!(!economy.admissions_open);
        assert_eq!(economy.unlimited_cash, Some(true));
        assert_eq!(economy.current_calendar_month, 13);
        assert_eq!(economy.lifetime_users, 5);
        assert_eq!(
            economy.monthly_finance_records[1].opening_cash,
            Money(100_000)
        );
        assert_eq!(economy.monthly_finance_records[1].active_users, 3);
        assert_eq!(economy.monthly_finance_records[1].total_users, 1);
        assert_eq!(
            economy.monthly_finance_records[1].admission_income,
            Money(5_000)
        );
        assert_eq!(
            economy.monthly_finance_records[1].construction,
            Money(10_000)
        );
        Ok(())
    }
}
