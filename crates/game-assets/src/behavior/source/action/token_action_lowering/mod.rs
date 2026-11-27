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
            ));
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
                        | "TimeToActivation"
                        | "TimeLeft"
                        | "Force"
                        | "Timeout"
                        | "Chance"
                        | "Reconsider"
                        | "Radius"
                        | "ChanceInRadius"
                        | "OnlyOne"
                        | "Payload"
                        | "RetainOnFailure"
                )
            })
        {
            return Err(invalid_behavior_source_data(
                "token delivery has unsupported attributes",
            ));
        }
        let radius_m = read_optional_float_attribute(token, "Radius")?;
        // A rampaging ankylosaur gives its target guest `t_InteractAnimal`
        // with `Radius="30"`; for the subject or target the radius is moot.
        let (recipient, nearby) = match (token.attribute("GiveTo"), radius_m) {
            (Some("subject"), _) => (BehaviorTokenRecipient::Subject, None),
            (Some("target"), _) => (BehaviorTokenRecipient::Target, None),
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
                ));
            }
        };
        if nearby.is_none() && token.attribute("ChanceInRadius").is_some() {
            return Err(invalid_behavior_source_data(
                "token ChanceInRadius requires a typed radius recipient",
            ));
        }
        let flag = |name| match token.attribute(name) {
            None | Some("false" | "0") => Ok(false),
            Some("true" | "1") => Ok(true),
            Some(_) => Err(invalid_behavior_source_data(format!(
                "token {name} must be true or false"
            ))),
        };
        let force = flag("Force")?;
        let reconsider = flag("Reconsider")? || force;
        let only_one = flag("OnlyOne")?;
        let retain_on_failure = flag("RetainOnFailure")?;
        let point_m = read_token_point(token)?;
        let payload = match token.attribute("Payload") {
            None => None,
            Some("subject") => Some(BehaviorTokenRecipient::Subject),
            Some("target") => Some(BehaviorTokenRecipient::Target),
            Some(_) => {
                return Err(invalid_behavior_source_data(
                    "token Payload must be subject or target",
                ));
            }
        };
        let name = read_optional_trimmed_string_attribute(token, "Name")
            .ok_or_else(|| invalid_behavior_source_data("token delivery requires Name"))?;
        let lifetime = match read_optional_float_attribute(token, "TimeLeft")?
            .or(read_optional_float_attribute(token, "Timeout")?)
        {
            None => None,
            Some(seconds) if seconds == UNBOUNDED_TOKEN_TIMEOUT_SECONDS => None,
            Some(seconds) => Some(Duration::try_from_secs_f64(f64::from(seconds)).map_err(
                |_| invalid_behavior_source_data("token Timeout must be finite and nonnegative"),
            )?),
        };
        let delay = Duration::try_from_secs_f64(f64::from(
            read_optional_float_attribute(token, "TimeToActivation")?
                .or(read_optional_float_attribute(token, "Timein")?)
                .unwrap_or(0.0),
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
            force,
            point_m,
            chance_percent,
            recipient,
            reconsider,
            nearby,
            only_one,
            payload,
            retain_on_failure,
        });
    }
    Ok(Some(BehaviorAction::SendSelfTokens(deliveries)))
}

fn read_token_point(token: &OrderedSourceDocumentNode) -> io::Result<Option<[f32; 3]>> {
    let mut point = None;
    for child in token.element_children() {
        let node = if child.name.eq_ignore_ascii_case("target") {
            child
        } else if child.name.eq_ignore_ascii_case("tokenData") {
            let mut children = child.element_children();
            let target = children
                .next()
                .filter(|node| node.name.eq_ignore_ascii_case("target"));
            if children.next().is_some() || target.is_none() {
                return Err(invalid_behavior_source_data("unsupported tokenData"));
            }
            target.expect("a target was validated above")
        } else {
            return Err(invalid_behavior_source_data("unsupported token child"));
        };
        if point.is_some() {
            return Err(invalid_behavior_source_data(
                "token has multiple target points",
            ));
        }
        let coordinate = |axis| {
            read_optional_float_attribute(node, axis)?
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    invalid_behavior_source_data(format!("token target requires finite {axis}"))
                })
        };
        point = Some([coordinate("x")?, coordinate("y")?, coordinate("z")?]);
    }
    Ok(point.filter(|point| *point != [-1.0; 3]).map(
        crate::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn dispatched_token_keeps_a_distinct_payload_point_force_and_window() {
        let document = parse_blue_fang_source_document(
            AssetPath::new("dispatch.beh"),
            br#"<behaviors><BFBehSendToken><BFAITokenList>
                <BFAIToken Name="t_Visit" GiveTo="target" Payload="subject" Force="1"
                            Timein="20" Timeout="30" TimeToActivation="2" TimeLeft="7">
                    <tokenData><target x="2" y="3" z="4"/></tokenData>
                </BFAIToken>
            </BFAITokenList></BFBehSendToken></behaviors>"#,
        )
        .expect("source");
        let Some(BehaviorAction::SendSelfTokens(rows)) =
            lower_self_token_delivery(document.root.element_children().next().expect("action"))
                .expect("lower token list")
        else {
            panic!("token list")
        };
        let row = &rows[0];
        assert_eq!(row.recipient, BehaviorTokenRecipient::Target);
        assert_eq!(row.payload, Some(BehaviorTokenRecipient::Subject));
        assert_eq!(row.point_m, Some([2.0, 4.0, -3.0]));
        assert!(row.force && row.reconsider);
        assert_eq!(row.delay, Duration::from_secs(2));
        assert_eq!(row.lifetime, Some(Duration::from_secs(7)));
    }

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
