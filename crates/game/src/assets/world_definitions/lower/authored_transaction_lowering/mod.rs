//! Resolves named transaction fields through the existing source type family.

use super::source_element_tree_search::authored_type_family_components;
use super::world_definition_source_value_reading_and_conversion::id;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::{
    canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
};
use openzt2_game_data::world_definitions::economy_transactions::{
    EconomyTransactionCostBasis, EconomyTransactionDefinition, EconomyTransactionKind,
    EconomyTransactionPeriod,
};
use std::collections::BTreeSet;

pub(super) fn lower_object_transactions(
    record: &RecordView<'_, '_>,
) -> Result<Vec<EconomyTransactionDefinition>, BindError> {
    let source: Vec<_> = authored_type_family_components(record, "ZTEconomyComponent")
        .into_iter()
        .flat_map(|component| component.element_children())
        .filter(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "ZTTransaction")
                || source_document_names_are_semantically_equal(
                    child.name.as_str(),
                    "BFGTransaction",
                )
        })
        .collect();
    let mut transactions = Vec::new();
    let mut names = BTreeSet::new();
    for transaction in &source {
        let Some(name) = transaction.attribute_named_any(&["name"]) else {
            return Err(BindError::record(record, "transaction has no name"));
        };
        let name_id = id(name);
        if !names.insert(name_id) {
            continue;
        }
        let attribute = |key| {
            source
                .iter()
                .filter(|candidate| {
                    candidate
                        .attribute_named_any(&["name"])
                        .is_some_and(|candidate| {
                            source_document_names_are_semantically_equal(candidate, name)
                        })
                })
                .find_map(|candidate| candidate.attribute_named_any(&[key]))
        };
        let number = |key, default| -> Result<f32, BindError> {
            let value = attribute(key)
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| {
                            BindError::record(record, format!("invalid transaction {name} {key}"))
                        })
                })
                .transpose()?
                .unwrap_or(default);
            Ok(value)
        };
        let boolean = |key, default| -> Result<bool, BindError> {
            attribute(key).map_or(Ok(default), |value| {
                match canonicalize_source_document_record_key(value).as_str() {
                    "true" | "1" | "yes" => Ok(true),
                    "false" | "0" | "no" => Ok(false),
                    _ => Err(BindError::record(
                        record,
                        format!("invalid transaction {name} {key}"),
                    )),
                }
            })
        };
        let kind = match attribute("type").unwrap_or("debit") {
            "debit" => EconomyTransactionKind::Debit,
            "credit" => EconomyTransactionKind::Credit,
            "setCash" => EconomyTransactionKind::SetCash,
            "addUser" => EconomyTransactionKind::AddUser,
            "removeUser" => EconomyTransactionKind::RemoveUser,
            value => {
                return Err(BindError::record(
                    record,
                    format!("unmapped transaction type {value}"),
                ));
            }
        };
        let cost_basis = match attribute("costType").unwrap_or("set") {
            "set" => EconomyTransactionCostBasis::Fixed,
            "parent" => EconomyTransactionCostBasis::Parent,
            "%parent" => EconomyTransactionCostBasis::PercentParent,
            "random" => EconomyTransactionCostBasis::Random,
            value => {
                return Err(BindError::record(
                    record,
                    format!("unmapped transaction costType {value}"),
                ));
            }
        };
        let period = match attribute("period").unwrap_or("demand") {
            "demand" => EconomyTransactionPeriod::Demand,
            "once" => EconomyTransactionPeriod::Once,
            "daily" => EconomyTransactionPeriod::Daily,
            "monthly" => EconomyTransactionPeriod::Monthly,
            "yearly" => EconomyTransactionPeriod::Yearly,
            value => {
                return Err(BindError::record(
                    record,
                    format!("unmapped transaction period {value}"),
                ));
            }
        };
        let cost_choices = attribute("costChoice")
            .unwrap_or_default()
            .split_whitespace()
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| BindError::record(record, "invalid transaction costChoice"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let initial_cost_index = attribute("costIndex")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| BindError::record(record, "invalid transaction costIndex"))?;
        if !cost_choices.is_empty() && initial_cost_index >= cost_choices.len() {
            return Err(BindError::record(
                record,
                "transaction costIndex exceeds costChoice",
            ));
        }
        transactions.push(EconomyTransactionDefinition {
            name: name_id,
            category: attribute("category").map(id).unwrap_or_default(),
            kind,
            cost_basis,
            cost: number("cost", 0.0)?,
            minimum_cost: number("minCost", 0.0)?,
            maximum_cost: number("maxCost", 0.0)?,
            cost_step: number("costStep", 0.0)?,
            cost_choices,
            initial_cost_index,
            period,
            frequency: attribute("frequency")
                .unwrap_or("1")
                .parse()
                .map_err(|_| BindError::record(record, "invalid transaction frequency"))?,
            aggregate: boolean("aggregate", true)?,
            track_on_parent: boolean("trackOnParent", true)?,
            target: attribute("target")
                .filter(|value| !value.trim().is_empty())
                .map(id),
            next_transaction: attribute("nextTransaction")
                .filter(|value| !value.trim().is_empty())
                .map(id),
        });
    }
    // Validate chains once during lowering, rather than allocating a visited set
    // each time a customer buys something.
    for transaction in &transactions {
        let mut next = transaction.next_transaction;
        for remaining in (0..transactions.len()).rev() {
            let Some(name) = next else {
                break;
            };
            let Some(target) = transactions.iter().find(|candidate| candidate.name == name) else {
                return Err(BindError::record(
                    record,
                    "transaction chain references missing operation",
                ));
            };
            if remaining == 0 {
                return Err(BindError::record(
                    record,
                    "transaction chain contains a cycle",
                ));
            }
            next = target.next_transaction;
        }
    }
    Ok(transactions)
}
