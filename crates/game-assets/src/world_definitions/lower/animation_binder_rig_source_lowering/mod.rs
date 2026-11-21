use std::collections::BTreeMap;

use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::RecordView,
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};
use openzt2_game_data::{
    world_definitions::animation_binder_rig::{
        AnimationBinderRig, DecalTextureCycle, DecalTextureStep, GroundFitLeg, GroundFitRig,
        GroundFitSupport, QuadrupedLegFit, SpineBendRig,
    },
    AssetId,
};

use super::super::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use super::source_element_tree_search::{authored_type_family_elements_named, find_descendant};

impl WorldDefinitionAsset {
    pub fn animation_binder_rigs(&self) -> &[AnimationBinderRig] {
        &self.document.animation_binder_rigs
    }
}

pub(super) fn lower_authored_animation_binder_rigs(
    record: &RecordView<'_, '_>,
) -> Vec<AnimationBinderRig> {
    let mut binder_components = BTreeMap::<AssetId, Vec<&OrderedSourceDocumentNode>>::new();
    for binder in authored_type_family_elements_named(record, "BFNamedBinder") {
        let Some(name) = binder.attribute_named_any(&["binderName"]) else {
            continue;
        };
        binder_components
            .entry(joint_id(name))
            .or_default()
            .push(binder);
    }
    let definition = joint_id(record.key);
    binder_components
        .into_iter()
        .filter_map(move |(binder, family)| {
            let component = |name| {
                family
                    .iter()
                    .find_map(|element| find_descendant(element, name))
            };
            let legs = component("BFLegFitComponent");
            let quadruped_ground_fit = component("BF4LegsGroundFitComponent")
                .map(|ground| lower_ground_fit(ground, legs, true));
            let biped_ground_fit = component("BF2LegsGroundFitComponent")
                .map(|ground| lower_ground_fit(ground, None, false));
            let spine_bend = component("BFSpineBendComponent").and_then(lower_spine_bend);
            let decal_cycles = family
                .iter()
                .map(|element| {
                    let mut cycles = Vec::new();
                    collect_decal_cycles(element, &mut cycles);
                    cycles
                })
                .find(|cycles| !cycles.is_empty())
                .unwrap_or_default();
            (quadruped_ground_fit.is_some()
                || biped_ground_fit.is_some()
                || spine_bend.is_some()
                || !decal_cycles.is_empty())
            .then_some(AnimationBinderRig {
                definition,
                binder,
                quadruped_ground_fit,
                biped_ground_fit,
                spine_bend,
                decal_cycles,
            })
        })
        .collect()
}

fn joint_id(name: &str) -> AssetId {
    AssetId::from_key(&canonicalize_source_document_record_key(name))
}

fn joint(node: &OrderedSourceDocumentNode, attribute: &str) -> Option<AssetId> {
    node.attribute_named_any(&[attribute])
        .filter(|value| !value.trim().is_empty())
        .map(joint_id)
}

fn number(node: &OrderedSourceDocumentNode, attribute: &str, default: f32) -> f32 {
    node.attribute_named_any(&[attribute])
        .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

fn flag(node: &OrderedSourceDocumentNode, attribute: &str, default: bool) -> bool {
    node.attribute_named_any(&[attribute])
        .map_or(default, |value| {
            value.trim() == "1" || value.trim().eq_ignore_ascii_case("true")
        })
}

fn source_axis(node: &OrderedSourceDocumentNode, attribute: &str) -> [f32; 3] {
    let mut values = node
        .attribute_named_any(&[attribute])
        .unwrap_or_default()
        .split_ascii_whitespace()
        .filter_map(parse_blue_fang_source_numeric_lexeme::<f32>);
    let x = values.next().unwrap_or_default();
    let y = values.next().unwrap_or_default();
    let z = values.next().unwrap_or_default();
    [x, z, -y]
}

fn lower_ground_fit(
    node: &OrderedSourceDocumentNode,
    legs: Option<&OrderedSourceDocumentNode>,
    quadruped: bool,
) -> GroundFitRig {
    let support = if quadruped {
        GroundFitSupport::Quadruped {
            bicycle_points: ["bicyclePointFront", "bicyclePointBack"].map(|name| joint(node, name)),
            foot_spacing: number(node, "footXSpacing", -1.0),
            legs: legs.map(|legs| QuadrupedLegFit {
                initially_enabled: flag(legs, "enable", true),
                maximum_raise: number(legs, "maxLegRaise", f32::MAX).abs(),
                legs: ["LeftFront", "RightFront", "LeftBack", "RightBack"].map(|leg| {
                    let prefix = leg.to_ascii_lowercase();
                    GroundFitLeg {
                        hip: joint(legs, &format!("{prefix}HipJoint")),
                        knee: joint(legs, &format!("{prefix}KneeJoint")),
                        ankle: joint(legs, &format!("{prefix}AnkleJoint")),
                        hip_axis: source_axis(legs, &format!("hipRaiseVector{leg}")),
                        knee_axis: source_axis(legs, &format!("kneeRaiseVector{leg}")),
                        ankle_axis: source_axis(legs, &format!("ankleRotVector{leg}")),
                        fit_mode: legs
                            .attribute_named_any(&[&format!("legFitMode{leg}")])
                            .and_then(parse_blue_fang_source_numeric_lexeme::<u8>)
                            .unwrap_or(0),
                    }
                }),
            }),
        }
    } else {
        GroundFitSupport::Biped {
            ankles: ["leftAnkleJoint", "rightAnkleJoint"].map(|name| joint(node, name)),
            ankle_axes: ["ankleRotVectorLeft", "ankleRotVectorRight"]
                .map(|name| source_axis(node, name)),
            fit_radius: number(node, "fitRadius", 0.0),
        }
    };
    GroundFitRig {
        support,
        height_offset: number(node, "heightOffset", 0.0),
        height_offset_scale: number(node, "heightOffsetScale", 1.0),
        height_offset_disabled_seconds: number(node, "heightOffsetDisabledCountdown", 0.0).max(0.0),
        tilt_off: flag(node, "tiltOff", false),
        allowed_roll_radians: number(node, "allowedRoll", 180.0).abs().to_radians(),
        error_limits: ["maxHeightError", "maxPitchError", "maxRollError"]
            .map(|name| number(node, name, f32::MAX).abs()),
        gains: [
            number(node, "heightGain", 6.0),
            number(node, "pitchGain", 1.0),
            number(node, "rollGain", 1.0),
        ],
        buoyancy: ["frontBuoyancyNode", "backBuoyancyNode"].map(|name| joint(node, name)),
    }
}

fn lower_spine_bend(node: &OrderedSourceDocumentNode) -> Option<SpineBendRig> {
    let bones = |prefix| {
        (1..=4)
            .filter_map(|index| joint(node, &format!("{prefix}{index}")))
            .collect::<Vec<_>>()
    };
    Some(SpineBendRig {
        base: joint(node, "spineBase")?,
        spine: bones("spineBone"),
        tail: bones("tailBone"),
        maximum_bend_radians: number(node, "bendDegree", 90.0).to_radians().abs(),
        bend_radius: node
            .attribute_named_any(&["bendRadiusMax"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|radius| radius.is_finite() && *radius > 0.0),
        blend_seconds: number(node, "bendTime", 0.0),
    })
}

fn collect_decal_cycles(node: &OrderedSourceDocumentNode, cycles: &mut Vec<DecalTextureCycle>) {
    if source_document_names_are_semantically_equal(
        node.name.as_str(),
        "BFDecalTextureCycleComponent",
    ) {
        if let Some(cycle) = lower_decal_cycle(node) {
            cycles.push(cycle);
        }
    } else {
        for element in node.element_children() {
            collect_decal_cycles(element, cycles);
        }
    }
}

fn lower_decal_cycle(node: &OrderedSourceDocumentNode) -> Option<DecalTextureCycle> {
    let material = node
        .attribute_named_any(&["materialName"])?
        .trim()
        .to_ascii_lowercase()
        .into_boxed_str();
    let layer = match node
        .attribute_named_any(&["layer"])
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "base" | "diffuse" => None,
        "decal1" => Some(2),
        "decal2" => Some(3),
        _ => Some(1),
    };
    let steps = node
        .element_children()
        .filter_map(|step| {
            if !source_document_names_are_semantically_equal(step.name.as_str(), "Step") {
                return None;
            }
            Some(DecalTextureStep {
                image: canonicalize_source_document_record_key(
                    step.attribute_named_any(&["imageName"])?,
                )
                .into_boxed_str(),
                hold_seconds: number(step, "hold", 0.0).max(0.0),
                variance_seconds: number(step, "variance", 0.0).abs(),
            })
        })
        .collect::<Vec<_>>();
    let stop = node
        .attribute_named_any(&["goto"])
        .and_then(parse_blue_fang_source_numeric_lexeme::<i32>);
    (!steps.is_empty()).then_some(DecalTextureCycle {
        material,
        layer,
        texture_coordinates: node
            .attribute_named_any(&["textureSet"])
            .and_then(parse_blue_fang_source_numeric_lexeme::<u32>),
        enabled: node
            .attribute_named_any(&["enabled"])
            .is_none_or(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")),
        frozen: stop == Some(-2),
        stop_at: stop.and_then(|stop| usize::try_from(stop).ok()),
        steps,
    })
}
