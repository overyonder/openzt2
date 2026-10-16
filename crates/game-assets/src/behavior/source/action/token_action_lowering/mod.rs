use std::{io, time::Duration};

use openzt2_game_data::{
    behavior::{action::token::BehaviorSelfTokenDelivery, action_record::BehaviorAction},
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_optional_float_attribute, read_optional_trimmed_string_attribute},
};

pub(super) fn lower_self_token_delivery(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Option<BehaviorAction>> {
    let list = match node.name.as_str() {
        "BFAITokenList" => node,
        "BFBehSendToken" if node.attributes.is_empty() => {
            let mut children = node.element_children();
            let Some(list) = children
                .next()
                .filter(|child| child.name == "BFAITokenList")
            else {
                return Err(invalid_behavior_source_data(
                    "BFBehSendToken requires a token list",
                ));
            };
            if children.next().is_some() {
                return Err(invalid_behavior_source_data(
                    "BFBehSendToken has unmapped children",
                ));
            }
            list
        }
        "BFBehSendToken" => {
            return Err(invalid_behavior_source_data(
                "BFBehSendToken has unmapped policy",
            ))
        }
        _ => return Ok(None),
    };
    if !list.attributes.is_empty() {
        return Err(invalid_behavior_source_data(
            "BFAITokenList has unmapped policy",
        ));
    }
    let mut deliveries = Vec::new();
    for token in list.element_children() {
        if token.name != "BFAIToken"
            || token.attributes.iter().any(|attribute| {
                !matches!(
                    attribute.name(),
                    "Name" | "GiveTo" | "Timein" | "Timeout" | "Chance"
                )
            })
            || token.element_children().next().is_some()
            || token.attribute("GiveTo") != Some("subject")
        {
            return Err(invalid_behavior_source_data(
                "token delivery requires a payload-free subject token",
            ));
        }
        let name = read_optional_trimmed_string_attribute(token, "Name")
            .ok_or_else(|| invalid_behavior_source_data("token delivery requires Name"))?;
        let seconds = read_optional_float_attribute(token, "Timeout")?.ok_or_else(|| {
            invalid_behavior_source_data("token delivery requires a finite Timeout")
        })?;
        let lifetime = Duration::try_from_secs_f64(f64::from(seconds)).map_err(|_| {
            invalid_behavior_source_data("token Timeout must be finite and nonnegative")
        })?;
        let delay = Duration::try_from_secs_f64(f64::from(
            read_optional_float_attribute(token, "Timein")?.unwrap_or(0.0),
        ))
        .map_err(|_| invalid_behavior_source_data("token Timein must be finite and nonnegative"))?;
        let chance_percent = read_optional_float_attribute(token, "Chance")?.unwrap_or(100.0);
        if !chance_percent.is_finite() || !(0.0..=100.0).contains(&chance_percent) {
            return Err(invalid_behavior_source_data(
                "token Chance must lie between zero and 100",
            ));
        }
        deliveries.push(BehaviorSelfTokenDelivery {
            token: AssetId::from_key(&name.to_ascii_lowercase()),
            delay,
            lifetime,
            chance_percent,
        });
    }
    Ok(Some(BehaviorAction::SendSelfTokens(deliveries)))
}
