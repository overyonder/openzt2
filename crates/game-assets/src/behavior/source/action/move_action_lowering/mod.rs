use std::io;

use openzt2_game_data::behavior::{
    action::movement::{BehaviorMoveAction, BehaviorPursuitAction},
    action::wander::BehaviorGroundLocomotionSpeed,
    action_record::BehaviorAction,
};

use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

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
    use crate::source_document::{
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
            Some(BehaviorGroundLocomotionSpeed::Fast)
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
    fn move_retains_the_authored_water_surface_depth() {
        let action =
            lower_one_authored_move_action(br#"<tasks><BFBehMove depthBelowSurface="4"/></tasks>"#);
        assert_eq!(action.depth_below_surface_m, Some(4.0));
        assert_eq!(action.height_above_floor_m, None);
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
                    // These keys do not change the move radius or floor clearance.
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
            .filter(|speed| !speed.eq_ignore_ascii_case("variable"))
            .map(|speed| BehaviorGroundLocomotionSpeed::from_authored(&speed));
    let water_height_constraint = |name| -> io::Result<Option<f32>> {
        match read_optional_float_attribute(move_action_source_node, name)? {
            None | Some(-1.0) => Ok(None),
            Some(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
            Some(_) => Err(invalid_behavior_source_data(format!(
                "BFBehMove has invalid {name}"
            ))),
        }
    };
    Ok(Some(BehaviorAction::Move(BehaviorMoveAction {
        locomotion_speed,
        target_node_name: read_optional_trimmed_string_attribute(
            move_action_source_node,
            "targetNode",
        ),
        move_radius_cm: read_optional_centimetres_attribute(move_action_source_node, "moveRadius")?
            .unwrap_or(200),
        depth_below_surface_m: water_height_constraint("depthBelowSurface")?,
        height_above_floor_m: water_height_constraint("heightAboveFloor")?,
    })))
}

pub(super) fn lower_pursuit_action_source_node(
    source_node: &DataNode,
) -> io::Result<Option<BehaviorAction>> {
    if source_node.name != "BFBehPursuit" {
        return Ok(None);
    }
    // Attribute names match exactly, so a differently cased key, such as an
    // authored `locospeed`, is ignored and its default applies. The original
    // game also ignores the avoidance keys on a pursuit.
    let pursuit_keys = [
        "locoSpeed",
        "hitRadius",
        "pursuitRadius",
        "failureRadius",
        "pollFreq",
    ];
    if source_node.attributes.iter().any(|attribute| {
        let name = attribute.name();
        !pursuit_keys
            .iter()
            .any(|key| key.eq_ignore_ascii_case(name))
            && !matches!(name, "avoidWater" | "avoidEntityRadius")
    }) || source_node.element_children().next().is_some()
    {
        return Err(invalid_behavior_source_data(
            "BFBehPursuit has unsupported authored policy",
        ));
    }
    // An absent speed means `slow`.
    let locomotion_speed = read_optional_trimmed_string_attribute(source_node, "locoSpeed")
        .map_or(BehaviorGroundLocomotionSpeed::Slow, |speed| {
            BehaviorGroundLocomotionSpeed::from_authored(&speed)
        });
    let poll_interval_seconds =
        read_optional_float_attribute(source_node, "pollFreq")?.unwrap_or(1.0);
    if !poll_interval_seconds.is_finite() {
        return Err(invalid_behavior_source_data(
            "BFBehPursuit has invalid pollFreq",
        ));
    }
    Ok(Some(BehaviorAction::Pursuit(BehaviorPursuitAction {
        locomotion_speed,
        hit_radius_cm: read_optional_centimetres_attribute(source_node, "hitRadius")?
            .unwrap_or(300),
        pursuit_radius_cm: read_optional_centimetres_attribute(source_node, "pursuitRadius")?
            .unwrap_or(1_200),
        failure_radius_cm: read_optional_centimetres_attribute(source_node, "failureRadius")?
            .unwrap_or(5_000),
        poll_interval_seconds,
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
