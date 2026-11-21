use std::{collections::BTreeSet, io};

use openzt2_game_data::{
    world_definitions::economy_transactions::{
        EconomyTransactionCostBasis, EconomyTransactionDefinition, EconomyTransactionKind,
        EconomyTransactionPeriod,
    },
    AssetId,
};

use crate::{
    source_document::{
        blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
        ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
        source_document_semantic_name::{
            canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
        },
    },
    world_definitions::world_definition_asset_set_refresh_and_indexing::saved_world_economy_asset::{
        SavedWorldEconomyAsset, SavedWorldEconomyTrackerDefinition,
    },
};

use super::super::starting_zoo::{at, local};

fn id(value: &str) -> AssetId {
    AssetId::from_key(&canonicalize_source_document_record_key(value))
}

pub(super) fn lower_economy_trackers(
    document: &OrderedSourceDocument,
) -> io::Result<SavedWorldEconomyAsset> {
    let mut trackers = Vec::new();
    let mut names = BTreeSet::new();
    for component in document.root.element_children().filter(|node| {
        matches!(
            local(&node.name),
            "ZTEconomyComponent" | "BFGEconomyComponent"
        )
    }) {
        let category = component
            .attribute_named_any(&["category"])
            .unwrap_or_default();
        let name =
            if source_document_names_are_semantically_equal(category, "generalEconomyComponent") {
                "general"
            } else {
                component
                    .attribute_named_any(&["name", "tracker", "category"])
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        at(
                            document,
                            component,
                            "economy tracker has no name or category",
                        )
                    })?
            };
        let name = id(name);
        if !names.insert(name) {
            return Err(at(document, component, "duplicate economy tracker"));
        }
        trackers.push(SavedWorldEconomyTrackerDefinition {
            name,
            transactions: lower_tracker_transactions(document, component)?,
        });
    }
    Ok(SavedWorldEconomyAsset {
        trackers: trackers.into_boxed_slice(),
    })
}

fn lower_tracker_transactions(
    document: &OrderedSourceDocument,
    component: &OrderedSourceDocumentNode,
) -> io::Result<Vec<EconomyTransactionDefinition>> {
    let source = component
        .element_children()
        .filter(|child| matches!(local(&child.name), "ZTTransaction" | "BFGTransaction"))
        .collect::<Vec<_>>();
    let mut transactions = Vec::new();
    let mut names = BTreeSet::new();
    for transaction in &source {
        let Some(name) = transaction.attribute_named_any(&["name"]) else {
            return Err(at(document, transaction, "transaction has no name"));
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
        let number = |key, default| -> Result<f32, io::Error> {
            let value = attribute(key)
                .map(|value| {
                    parse_blue_fang_source_numeric_lexeme::<f32>(value)
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| {
                            at(
                                document,
                                transaction,
                                format!("invalid transaction {name} {key}"),
                            )
                        })
                })
                .transpose()?
                .unwrap_or(default);
            Ok(value)
        };
        let boolean = |key, default| -> Result<bool, io::Error> {
            attribute(key).map_or(Ok(default), |value| {
                match canonicalize_source_document_record_key(value).as_str() {
                    "true" | "1" | "yes" => Ok(true),
                    "false" | "0" | "no" => Ok(false),
                    _ => Err(at(
                        document,
                        transaction,
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
                return Err(at(
                    document,
                    transaction,
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
                return Err(at(
                    document,
                    transaction,
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
                return Err(at(
                    document,
                    transaction,
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
                    .ok_or_else(|| at(document, transaction, "invalid transaction costChoice"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let initial_cost_index = attribute("costIndex")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| at(document, transaction, "invalid transaction costIndex"))?;
        if !cost_choices.is_empty() && initial_cost_index >= cost_choices.len() {
            return Err(at(
                document,
                transaction,
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
                .map_err(|_| at(document, transaction, "invalid transaction frequency"))?,
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
                return Err(at(
                    document,
                    component,
                    "transaction chain references missing operation",
                ));
            };
            if remaining == 0 {
                return Err(at(
                    document,
                    component,
                    "transaction chain contains a cycle",
                ));
            }
            next = target.next_transaction;
        }
    }
    Ok(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn general_and_named_trackers_keep_separate_transactions(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let source = parse_blue_fang_source_document(
            AssetPath::new("config/economy_test.xml"),
            br#"<ZTEconomyMgr active="true">
                <ZTEconomyComponent category="generalEconomyComponent">
                    <ZTTransaction name="admission" type="credit" cost="3"/>
                </ZTEconomyComponent>
                <ZTEconomyComponent category="customEconomyComponent" name="Tour">
                    <ZTTransaction name="admission" type="debit" cost="7"/>
                </ZTEconomyComponent>
            </ZTEconomyMgr>"#,
        )?;
        let asset = lower_economy_trackers(&source)?;
        let [general, tour] = asset.trackers.as_ref() else {
            panic!("expected two trackers");
        };
        assert_eq!(general.name, AssetId::from_key("general"));
        assert_eq!(tour.name, AssetId::from_key("tour"));
        assert_eq!(general.transactions[0].kind, EconomyTransactionKind::Credit);
        assert_eq!(tour.transactions[0].kind, EconomyTransactionKind::Debit);
        assert_eq!(general.transactions[0].cost, 3.0);
        assert_eq!(tour.transactions[0].cost, 7.0);
        Ok(())
    }
}
