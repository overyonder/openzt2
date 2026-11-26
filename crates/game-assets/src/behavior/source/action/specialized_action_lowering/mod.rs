use std::io;

use openzt2_game_data::{
    behavior::{
        action::{specialized::*, wander::BehaviorGroundLocomotionSpeed},
        action_record::BehaviorAction,
    },
    AssetId,
};

use super::{
    invalid_behavior_source_data,
    source_value_reading::{
        read_boolean_attribute_or_false, read_optional_asset_identifier_attribute,
        read_optional_float_attribute, read_optional_trimmed_string_attribute,
    },
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode as DataNode;

pub(super) fn lower_specialized_action(node: &DataNode) -> io::Result<Option<BehaviorAction>> {
    let text = |name| read_optional_trimmed_string_attribute(node, name);
    let id = |name| read_optional_asset_identifier_attribute(node, name);
    let boolean = |name| read_boolean_attribute_or_false(node, name);
    let number = |name| {
        let value = read_optional_float_attribute(node, name)?;
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(invalid_behavior_source_data(format!(
                "{} has invalid {name}",
                node.name
            )));
        }
        Ok(value)
    };
    let speed = || {
        node.attribute_named_any(&["locoSpeed"]).map_or(
            BehaviorGroundLocomotionSpeed::Slow,
            BehaviorGroundLocomotionSpeed::from_authored,
        )
    };
    let action = match node.name.as_str() {
        "BFBehEscapeObstacle" => {
            let default = BehaviorEscapeObstacleAction::default();
            let time = number("escapeTime")?
                .map(std::time::Duration::try_from_secs_f32)
                .transpose()
                .map_err(|_| invalid_behavior_source_data("invalid escapeTime"))?
                .unwrap_or(default.time);
            let radius = read_optional_float_attribute(node, "escapeRadius")?;
            if radius.is_some_and(|radius| !radius.is_finite() || (radius < 0.0 && radius != -1.0))
            {
                return Err(invalid_behavior_source_data("invalid escapeRadius"));
            }
            BehaviorAction::EscapeObstacle(BehaviorEscapeObstacleAction {
                speed: speed(),
                time,
                radius_metres: radius.filter(|radius| *radius >= 0.0),
            })
        }
        "BFBehExit" => BehaviorAction::ExitInteractionContainer {
            exit_behavior_set: id("exitBehSet"),
        },
        "ZTBehFindArtifact" => BehaviorAction::FindArtifact,
        "ZTBehDigArtifact" => BehaviorAction::DigArtifact,
        "ZTBehTour" => BehaviorAction::Tour,
        "ZTBehTeleportToLoc" => {
            BehaviorAction::TeleportToLocation(BehaviorTeleportToLocationAction {
                position: text("position")
                    .map(|value| parse_splash_vector(&value))
                    .transpose()?,
                reset_prop: boolean("resetProp")?,
            })
        }
        "ZTBehKickOffRider" => BehaviorAction::KickOffRider,
        "ZTBehAnimateForViewFromTransport" => BehaviorAction::AnimateForViewFromTransport {
            animation_type: text("animationType").ok_or_else(|| {
                invalid_behavior_source_data(
                    "ZTBehAnimateForViewFromTransport requires animationType",
                )
            })?,
        },
        "BFBehFall" => {
            let speed_mps = read_optional_float_attribute(node, "fallSpeed")?.unwrap_or_default();
            if !speed_mps.is_finite() {
                return Err(invalid_behavior_source_data(
                    "BFBehFall has invalid fallSpeed",
                ));
            }
            BehaviorAction::Fall(BehaviorFallAction {
                speed_mps,
                airborne: text("fallAnim"),
                land_impact: text("landImpactAnim"),
                water_impact: text("waterImpactAnim"),
            })
        }
        "BFBehDockNow" => BehaviorAction::DockNow(BehaviorDockNowAction {
            subject_node: text("subjectNode"),
            target_node: text("targetNode"),
            target_animation: text("targetAnim"),
            reservation: id("reserveSlotName"),
            redock: boolean("redock")?,
            parent_to_target: boolean("parentToTarget")?,
            unparent: boolean("unparent")?,
        }),
        "BFBehDockSpline" => BehaviorAction::DockSpline(BehaviorDockSplineAction {
            subject_node: text("subjectNode"),
            target_node: text("targetNode"),
            target_animation: text("targetAnim"),
            speed: speed(),
            ignore_target_collision: boolean("ignoreCollisionWithTarget")?,
            use_buoy_nodes: node.attribute("useBuoyNodes").is_none() || boolean("useBuoyNodes")?,
            redock: boolean("redock")?,
        }),
        "BFBehFollowEntity" => {
            let minimum = number("minPlayTime")?.unwrap_or_default();
            let maximum = number("maxPlayTime")?.unwrap_or(minimum);
            if maximum < minimum {
                return Err(invalid_behavior_source_data(
                    "follow duration bounds are reversed",
                ));
            }
            BehaviorAction::FollowEntity(BehaviorFollowEntityAction {
                subject_node: text("subjectNode"),
                target_node: text("targetNode"),
                speed: speed(),
                play_seconds: [minimum, maximum],
            })
        }
        "ZTBehSplashGuest" => BehaviorAction::SplashGuest(BehaviorSplashGuestAction {
            animation: text("targetAnim"),
            particle: text("splashName"),
            direction: text("splashVector")
                .map(|direction| parse_splash_vector(&direction))
                .transpose()?,
            interrupts_animation: boolean("interruptFlag")?,
            ground_fit: node
                .attribute("groundFit")
                .map(|_| boolean("groundFit"))
                .transpose()?,
            from_water_surface: node.attribute("splashFromWaterSurface").is_none()
                || boolean("splashFromWaterSurface")?,
            tokens: node
                .element_children()
                .find(|child| child.name == "BFAITokenList")
                .map(super::token_action_lowering::lower_self_token_delivery)
                .transpose()?
                .flatten()
                .and_then(|action| match action {
                    BehaviorAction::SendSelfTokens(tokens) => Some(tokens),
                    _ => None,
                })
                .unwrap_or_default(),
            show_event: node
                .element_children()
                .find(|child| child.name == "ZTAIShowEvent")
                .map(lower_splash_show_event)
                .transpose()?,
        }),
        "ZTBehUsePortal" => BehaviorAction::UsePortal(BehaviorUsePortalAction {
            animation: text("targetAnim"),
            speed: speed(),
        }),
        "ZTBehDockTankWall" => BehaviorAction::DockTankWall(BehaviorDockTankWallAction {
            subject_node: text("subjectNode"),
            speed: speed(),
            height_above_ground: number("heightAboveGround")?,
            depth_below_water: number("depthBelowWater")?,
        }),
        "ZTBehStaffTrainAnimal" => {
            BehaviorAction::StaffTrainAnimal(BehaviorStaffTrainAnimalAction {
                increment: number("IncrementAmt")?.unwrap_or_default(),
                summon_timeout: number("SummonAnimalTimeout")?.unwrap_or_default(),
                trick_timeout: number("AnimalDoesTrickTimeout")?.unwrap_or_default(),
                require_platform: boolean("requirePlatform")?,
                go_to_platform: id("GoToPlatformPrimaryBeh"),
                prepare: id("PreTrainingPrimaryBeh"),
                summon: id("SummonAnimalPrimaryBeh"),
                await_animal: id("SummonAnimalLoopingBeh"),
                train: id("TrainAnimalPrimaryBeh"),
                perform_trick: id("AnimalDoesTrickPrimaryBeh"),
                await_trick: id("AnimalDoesTrickLoopingBeh"),
                reward: id("RewardAnimalPrimaryBeh"),
                cleanup: id("CleanupPrimaryBeh"),
            })
        }
        "ZTBehAddSubjectAsDiseaseTarget" => {
            BehaviorAction::AddSubjectAsDiseaseTarget(BehaviorAddSubjectAsDiseaseTargetAction {
                disease: id("disease")
                    .ok_or_else(|| invalid_behavior_source_data("missing disease name"))?,
                types: node
                    .attribute("types")
                    .into_iter()
                    .flat_map(|types| types.split([',', ';', ' ']))
                    .filter(|name| !name.is_empty())
                    .map(AssetId::from_key)
                    .collect(),
                attributes: node
                    .attribute("attributes")
                    .into_iter()
                    .flat_map(|attributes| attributes.split([',', ';', ' ']))
                    .filter(|attribute| !attribute.trim().is_empty())
                    .map(parse_disease_attribute)
                    .collect::<io::Result<Vec<_>>>()?,
                immediate: boolean("immediate")?,
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(action))
}

fn parse_splash_vector(value: &str) -> io::Result<[f32; 3]> {
    let values = value
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid_behavior_source_data("invalid splash vector"))?;
    let [x, y, z] = values.as_slice() else {
        return Err(invalid_behavior_source_data(
            "splash vector must have three coordinates",
        ));
    };
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return Err(invalid_behavior_source_data("splash vector must be finite"));
    }
    Ok([*x, *z, -*y])
}

fn parse_disease_attribute(value: &str) -> io::Result<(AssetId, bool)> {
    let (name, value) = value.split_once('=').unwrap_or((value, "true"));
    let value = match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => {
            return Err(invalid_behavior_source_data(
                "invalid disease qualifier value",
            ));
        }
    };
    Ok((AssetId::from_key(name.trim()), value))
}

fn lower_splash_show_event(
    node: &DataNode,
) -> io::Result<openzt2_game_data::behavior::action::show_event::BehaviorShowEventAction> {
    let name = read_optional_asset_identifier_attribute(node, "Name")
        .ok_or_else(|| invalid_behavior_source_data("splash show event requires Name"))?;
    Ok(
        openzt2_game_data::behavior::action::show_event::BehaviorShowEventAction {
            name,
            priority: node
                .attribute("Priority")
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
            interrupt: read_boolean_attribute_or_false(node, "Interrupt")?,
            expire_with_task: read_boolean_attribute_or_false(node, "ExpireWithTask")?,
            time_in_seconds: read_optional_float_attribute(node, "Timein")?,
            timeout_seconds: read_optional_float_attribute(node, "Timeout")?,
        },
    )
}
