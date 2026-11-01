use std::{io, time::Duration};

use openzt2_game_data::{
    behavior::{
        action::token::{
            BehaviorNearbyTokenRecipients, BehaviorSelfTokenDelivery, BehaviorTokenRecipient,
        },
        action_record::BehaviorAction,
    },
    AssetId,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{read_optional_float_attribute, read_optional_trimmed_string_attribute},
};

/// The authored timeout that, like an absent one, never expires.
const UNBOUNDED_TOKEN_TIMEOUT_SECONDS: f32 = -1.0;

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
                    "Name"
                        | "GiveTo"
                        | "Timein"
                        | "Timeout"
                        | "Chance"
                        | "Reconsider"
                        | "Radius"
                        | "ChanceInRadius"
                        | "OnlyOne"
                )
            })
            || token.element_children().next().is_some()
        {
            return Err(invalid_behavior_source_data(
                "token delivery requires a payload-free token",
            ));
        }
        let radius_m = read_optional_float_attribute(token, "Radius")?;
        let (recipient, nearby) = match (token.attribute("GiveTo"), radius_m) {
            (Some("subject"), None) => (BehaviorTokenRecipient::Subject, None),
            (Some("target"), None) => (BehaviorTokenRecipient::Target, None),
            (Some(entity_type), Some(radius_m))
                if !matches!(entity_type, "subject" | "target")
                    && radius_m.is_finite()
                    && radius_m > 0.0 =>
            {
                let chance_percent =
                    read_optional_float_attribute(token, "ChanceInRadius")?.unwrap_or(100.0);
                if !chance_percent.is_finite() || !(0.0..=100.0).contains(&chance_percent) {
                    return Err(invalid_behavior_source_data(
                        "token ChanceInRadius must lie between zero and 100",
                    ));
                }
                (
                    BehaviorTokenRecipient::Subject,
                    Some(BehaviorNearbyTokenRecipients {
                        entity_type: AssetId::from_key(&entity_type.trim().to_ascii_lowercase()),
                        radius_m,
                        chance_percent,
                    }),
                )
            }
            _ => {
                return Err(invalid_behavior_source_data(
                    "token delivery requires a subject, target or typed radius recipient",
                ))
            }
        };
        if nearby.is_none() && token.attribute("ChanceInRadius").is_some() {
            return Err(invalid_behavior_source_data(
                "token ChanceInRadius requires a typed radius recipient",
            ));
        }
        let flag = |name| match token.attribute(name) {
            None | Some("false") => Ok(false),
            Some("true") => Ok(true),
            Some(_) => Err(invalid_behavior_source_data(format!(
                "token {name} must be true or false"
            ))),
        };
        let reconsider = flag("Reconsider")?;
        let only_one = flag("OnlyOne")?;
        let name = read_optional_trimmed_string_attribute(token, "Name")
            .ok_or_else(|| invalid_behavior_source_data("token delivery requires Name"))?;
        let lifetime = match read_optional_float_attribute(token, "Timeout")? {
            None => None,
            Some(seconds) if seconds == UNBOUNDED_TOKEN_TIMEOUT_SECONDS => None,
            Some(seconds) => Some(Duration::try_from_secs_f64(f64::from(seconds)).map_err(
                |_| invalid_behavior_source_data("token Timeout must be finite and nonnegative"),
            )?),
        };
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
            recipient,
            reconsider,
            nearby,
            only_one,
        });
    }
    Ok(Some(BehaviorAction::SendSelfTokens(deliveries)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn disease_spreads_to_one_typed_animal_in_radius() {
        let document = parse_blue_fang_source_document(AssetPath::new("disease.beh"),
            br#"<behaviors><BFBehSendToken><BFAITokenList><BFAIToken Name="t_GetSlowingDisease" GiveTo="b_ExtinctAnimal" Radius="15" ChanceInRadius="50" Timeout="60" Reconsider="true" OnlyOne="true"/></BFAITokenList></BFBehSendToken></behaviors>"#).expect("source");
        let Some(BehaviorAction::SendSelfTokens(deliveries)) =
            lower_self_token_delivery(document.root.element_children().next().expect("action"))
                .expect("lower")
        else {
            panic!("token delivery")
        };
        let nearby = deliveries[0].nearby.expect("radius recipients");
        assert_eq!(nearby.entity_type, AssetId::from_key("b_extinctanimal"));
        assert_eq!((nearby.radius_m, nearby.chance_percent), (15.0, 50.0));
        assert!(deliveries[0].only_one && deliveries[0].reconsider);
    }
}
