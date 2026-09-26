use std::io;

use openzt2_game_data::behavior::{
    action::movement::{BehaviorMoveAction, BehaviorMoveLocomotionSpeed},
    action_record::BehaviorAction,
};

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_float_attribute,
        read_optional_trimmed_string_attribute,
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    fn lower_one_authored_action(authored_source: &[u8]) -> io::Result<Option<BehaviorAction>> {
        let source = parse_blue_fang_source_document(AssetPath::new("tasks.tsk"), authored_source)
            .expect("source");
        lower_move_action_source_node(source.root.element_children().next().expect("action"))
    }

    fn lower_one_authored_move_action(authored_source: &[u8]) -> BehaviorMoveAction {
        match lower_one_authored_action(authored_source).expect("lower") {
            Some(BehaviorAction::Move(move_action)) => move_action,
            _ => panic!("move action"),
        }
    }

    #[test]
    fn bare_bfbehmove_lowers_to_the_native_field_defaults() {
        let move_action = lower_one_authored_move_action(br#"<tasks><BFBehMove/></tasks>"#);
        assert_eq!(move_action.target_node_name, None);
        // Native default from `construct_BFBehMove_members`: moveRadius 2.0 m.
        assert_eq!(move_action.move_radius_cm, 200);
        assert_eq!(move_action.locomotion_speed, None);
    }

    #[test]
    fn authored_keeper_feedanimal_fast_move_lowers_without_hardcoded_speed() {
        let move_action =
            lower_one_authored_move_action(br#"<tasks><BFBehMove locoSpeed="fast"/></tasks>"#);
        // The named mode is retained on the record; the record carries no
        // numeric speed because live speed is owned by NavAgent.
        assert_eq!(
            move_action.locomotion_speed,
            Some(BehaviorMoveLocomotionSpeed::Fast)
        );
        assert_eq!(move_action.move_radius_cm, 200);
    }

    #[test]
    fn authored_radius_and_target_node_lower() {
        let move_action = lower_one_authored_move_action(
            br#"<tasks><BFBehMove moveRadius="5" targetNode="p_InvestigateNode"/></tasks>"#,
        );
        assert_eq!(move_action.move_radius_cm, 500);
        assert_eq!(
            move_action.target_node_name.as_deref(),
            Some("p_InvestigateNode")
        );
    }

    #[test]
    fn native_default_route_policies_and_ignored_fields_are_preserved() {
        assert!(lower_one_authored_action(
            br#"<tasks><BFBehMove pathRadius="0" closestApproach="false" depthBelowSurface="-1" heightAboveFloor="-1" hitRadius="8" depthAboveBottom="1"/></tasks>"#,
        )
        .is_ok());
    }
}
pub(super) fn lower_move_action_source_node(
    move_action_source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if move_action_source_node.name != "BFBehMove" {
        return Ok(None);
    }
    if move_action_source_node.attributes.iter().any(|attribute| {
        !matches!(
            attribute.name(),
            "locoSpeed"
                    | "moveRadius"
                    | "targetNode"
                    | "pathRadius"
                    | "closestApproach"
                    | "depthBelowSurface"
                    | "heightAboveFloor"
                    // Native read_BFBehMove_xml has no bindings for these;
                    // preserve its observed no-op behavior.
                    | "hitRadius"
                    | "depthAboveBottom"
        )
    }) || move_action_source_node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehMove has unsupported authored policy",
        ));
    }
    validate_supported_bfbehmove_default_policies(move_action_source_node)?;
    let locomotion_speed =
        read_optional_trimmed_string_attribute(move_action_source_node, "locoSpeed")
            .map(
                |authored_speed| match authored_speed.to_ascii_lowercase().as_str() {
                    "variable" => Ok(None),
                    // The winning Keeper `fast` action resolves through the
                    // native empty/default locomotion fallback when the active
                    // land table has no `fast` child. The live executor therefore
                    // uses the actor's canonical NavAgent speed, without a
                    // guessed multiplier.
                    "fast" => Ok(Some(BehaviorMoveLocomotionSpeed::Fast)),
                    "slow" | "medium" | "sonar" => Err(invalid_behavior_source_data(
                        "BFBehMove locoSpeed has no live NavAgent mode mapping",
                    )),
                    _ => Err(invalid_behavior_source_data(
                        "BFBehMove has unsupported locoSpeed",
                    )),
                },
            )
            .transpose()?
            .flatten();
    Ok(Some(BehaviorAction::Move(BehaviorMoveAction {
        locomotion_speed,
        target_node_name: read_optional_trimmed_string_attribute(
            move_action_source_node,
            "targetNode",
        ),
        move_radius_cm: read_optional_centimetres_attribute(move_action_source_node, "moveRadius")?
            .unwrap_or(200),
    })))
}

fn validate_supported_bfbehmove_default_policies(source_node: &DataNode) -> io::Result<()> {
    if read_optional_float_attribute(source_node, "pathRadius")?.is_some_and(|value| value != 0.0) {
        return Err(invalid_behavior_source_data(
            "BFBehMove pathRadius has no live route-width mapping",
        ));
    }
    if read_boolean_attribute_or_false(source_node, "closestApproach")? {
        return Err(invalid_behavior_source_data(
            "BFBehMove closestApproach has unrecovered live semantics",
        ));
    }
    for attribute_name in ["depthBelowSurface", "heightAboveFloor"] {
        if read_optional_float_attribute(source_node, attribute_name)?
            .is_some_and(|value| value != -1.0)
        {
            return Err(invalid_behavior_source_data(format!(
                "BFBehMove {attribute_name} has no live terrain constraint mapping"
            )));
        }
    }
    Ok(())
}

fn read_optional_centimetres_attribute(
    source_node: &DataNode,
    attribute_name: &str,
) -> io::Result<Option<u32>> {
    source_node
        .attribute(attribute_name)
        .map(|metres| {
            metres
                .trim_end_matches(['f', 'F'])
                .parse::<f32>()
                .map_err(|_| {
                    invalid_behavior_source_data(format!(
                        "{} has invalid {attribute_name}",
                        source_node.name
                    ))
                })
                .and_then(|metres| {
                    let centimetres = (metres * 100.0).round();
                    if !metres.is_finite()
                        || metres < 0.0
                        || !centimetres.is_finite()
                        || f64::from(centimetres) > f64::from(u32::MAX)
                    {
                        return Err(invalid_behavior_source_data(format!(
                            "{} has unsupported {attribute_name}: expected finite nonnegative metres representable as u32 centimetres",
                            source_node.name
                        )));
                    }
                    Ok(centimetres as u32)
                })
        })
        .transpose()
}
