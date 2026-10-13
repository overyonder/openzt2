//! NetImmerse object controller chains lowered into prefab transform motion.

use std::collections::BTreeSet;

use bevy::math::Quat;
use openzt2_game_data::scene_prefab::{
    PrefabControllerClock, PrefabRotationCurve, PrefabRotationKey, PrefabScalarCurve,
    PrefabScalarCurveSegment, PrefabTransformAnimation,
};

use super::{
    super::{
        kf_animation_curve_sampling::{sample_netimmerse_keyframes, NetImmerseSourceKeyframe},
        netimmerse_nif_source::{
            animation_controller_source_types::{
                NetImmerseNiKeyframeController, NetImmerseNiKeyframeData,
            },
            block_payload::NetImmerseNifBlockPayload,
            document_source_types::NetImmerseNifDocument,
            interpolated_key_source_types::{
                NetImmerseFloatKey, NetImmerseFloatKeyGroup, NetImmerseQuaternionKey,
                NetImmerseVector3KeyGroup,
            },
            scene_object_source_types::NetImmerseNiAvObject,
        },
        netimmerse_texture_coordinate_animation_lowering::lower_scalar_track,
    },
    native_scene_lowering_error::NativeSceneLoweringError,
};

type Result<T> = std::result::Result<T, NativeSceneLoweringError>;

const NETIMMERSE_CONTROLLER_ACTIVE_FLAG: u16 = 1 << 3;
const NETIMMERSE_XYZ_ROTATION_KEY_TYPE: u32 = 4;

/// What an object's first transform controller does to its local transform.
pub(super) enum NetImmerseObjectTransformMotion {
    None,
    RotationCycle([f32; 3]),
    /// Every keyed channel holds one value for the whole controller interval,
    /// so the controller only replaces the authored pose.
    ConstantPose {
        translation_m: Option<[f32; 3]>,
        rotation_xyzw: Option<[f32; 4]>,
        uniform_scale: Option<f32>,
    },
    Animation(Box<PrefabTransformAnimation>),
}

pub(super) fn lower_netimmerse_object_transform_motion(
    document: &NetImmerseNifDocument,
    object: &NetImmerseNiAvObject,
) -> Result<NetImmerseObjectTransformMotion> {
    let Some(controller) = first_keyframe_controller_in_object_chain(document, object)? else {
        return Ok(NetImmerseObjectTransformMotion::None);
    };
    let duration = controller.controller.stop_time - controller.controller.start_time;
    if controller.controller.flags & NETIMMERSE_CONTROLLER_ACTIVE_FLAG == 0 || duration <= 0.0 {
        return Ok(NetImmerseObjectTransformMotion::None);
    }
    let data_block = document.block(controller.data_ref).ok_or_else(|| {
        NativeSceneLoweringError::new(
            &document.source_path,
            format!(
                "references missing NIF keyframe data block {}",
                controller.data_ref
            ),
        )
    })?;
    let NetImmerseNifBlockPayload::NiKeyframeData(data) = &data_block.payload else {
        return Err(NativeSceneLoweringError::new(
            &document.source_path,
            format!(
                "keyframe controller references {:?} instead of NiKeyframeData",
                data_block.payload
            ),
        ));
    };
    if let Some(speed) = constant_xyz_rotation_cycle(controller, data, duration) {
        return Ok(NetImmerseObjectTransformMotion::RotationCycle(speed));
    }
    lower_netimmerse_keyframe_data_transform_motion(&document.source_path, controller, data)
}

fn first_keyframe_controller_in_object_chain<'document>(
    document: &'document NetImmerseNifDocument,
    object: &NetImmerseNiAvObject,
) -> Result<Option<&'document NetImmerseNiKeyframeController>> {
    let mut controller_reference = object.object.controller_ref;
    let mut visited = BTreeSet::new();
    loop {
        if controller_reference < 0 {
            return Ok(None);
        }
        if !visited.insert(controller_reference) {
            return Err(NativeSceneLoweringError::new(
                &document.source_path,
                "contains a cyclic NIF object controller chain",
            ));
        }
        let block = document.block(controller_reference).ok_or_else(|| {
            NativeSceneLoweringError::new(
                &document.source_path,
                format!("references missing NIF controller block {controller_reference}"),
            )
        })?;
        match &block.payload {
            NetImmerseNifBlockPayload::NiKeyframeController(value) => return Ok(Some(value)),
            NetImmerseNifBlockPayload::NiUVController(value) => {
                // UV controllers are lowered with their geometry material and
                // evaluated through its persistent texture-transform buffer.
                let target = document.block(value.controller.target_ref).ok_or_else(|| {
                    NativeSceneLoweringError::new(
                        &document.source_path,
                        "missing UV controller target",
                    )
                })?;
                if !matches!(
                    target.payload,
                    NetImmerseNifBlockPayload::NiTriShape(_)
                        | NetImmerseNifBlockPayload::NiTriStrips(_)
                ) {
                    return Err(NativeSceneLoweringError::new(
                        &document.source_path,
                        "UV controller target is not supported geometry",
                    ));
                }
                controller_reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiVisController(value)
                if value.controller.flags & NETIMMERSE_CONTROLLER_ACTIVE_FLAG == 0 =>
            {
                // An inactive visibility controller does not animate visibility,
                // but its linked successor can still own a transform controller.
                controller_reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiBoneLODController(_) => return Ok(None),
            other => {
                return Err(NativeSceneLoweringError::new(
                    &document.source_path,
                    format!(
                        "uses unsupported object controller {other:?} at block {controller_reference}"
                    ),
                ));
            }
        }
    }
}

/// The globe models author one XYZ rotation key per axis as one full turn
/// per controller cycle.
fn constant_xyz_rotation_cycle(
    controller: &NetImmerseNiKeyframeController,
    data: &NetImmerseNiKeyframeData,
    duration: f32,
) -> Option<[f32; 3]> {
    if data.rotation_type != Some(NETIMMERSE_XYZ_ROTATION_KEY_TYPE)
        || data.xyz_rotations.len() != 3
        || data.xyz_rotations.iter().any(|axis| axis.keys.len() != 1)
        || !data.translations.keys.is_empty()
        || !data.scales.keys.is_empty()
    {
        return None;
    }
    let turn: [f32; 3] = std::array::from_fn(|axis| data.xyz_rotations[axis].keys[0].value);
    let scale = controller.controller.frequency / duration;
    Some([-turn[0] * scale, -turn[2] * scale, -turn[1] * scale])
}

fn lower_netimmerse_keyframe_data_transform_motion(
    source_path: &str,
    controller: &NetImmerseNiKeyframeController,
    data: &NetImmerseNiKeyframeData,
) -> Result<NetImmerseObjectTransformMotion> {
    let lower_curve = |group: &NetImmerseFloatKeyGroup| {
        lower_netimmerse_float_key_group_to_prefab_curve(source_path, group)
    };
    let translation_m = if data.translations.keys.is_empty() {
        None
    } else {
        let [x, y, z] = std::array::from_fn(|axis| {
            float_key_group_from_vector3_component(&data.translations, axis)
        });
        // Source [X, Y, Z] becomes Bevy [X, Z, -Y].
        Some([
            lower_curve(&x)?,
            lower_curve(&z)?,
            negate_prefab_curve(lower_curve(&y)?),
        ])
    };
    let rotation = match data.rotation_type {
        None => None,
        Some(NETIMMERSE_XYZ_ROTATION_KEY_TYPE) => {
            if data.xyz_rotations.len() != 3 {
                return Err(NativeSceneLoweringError::new(
                    source_path,
                    "NIF XYZ rotation keys do not have three axes",
                ));
            }
            // A source XYZ key is the inverse of Z*Y*X in the basis used for
            // authored node matrices. That is X(-x), Y(-y), Z(-z), which in
            // Bevy axes is X(-x), Z(y), Y(-z).
            Some(PrefabRotationCurve::EulerAnglesXzy([
                negate_prefab_curve(lower_curve(&data.xyz_rotations[0])?),
                lower_curve(&data.xyz_rotations[1])?,
                negate_prefab_curve(lower_curve(&data.xyz_rotations[2])?),
            ]))
        }
        Some(rotation_type) => Some(lower_netimmerse_quaternion_keys(
            source_path,
            rotation_type,
            &data.quaternion_keys,
        )?),
    };
    let uniform_scale = if data.scales.keys.is_empty() {
        None
    } else {
        Some(lower_curve(&data.scales)?)
    };
    let constant_translation = translation_m.as_ref().map(|curves| {
        curves
            .iter()
            .map(constant_prefab_curve_value)
            .collect::<Option<Vec<_>>>()
    });
    let constant_rotation = rotation.as_ref().map(constant_prefab_rotation);
    let constant_scale = uniform_scale.as_ref().map(constant_prefab_curve_value);
    if constant_translation.as_ref().is_none_or(Option::is_some)
        && constant_rotation.as_ref().is_none_or(Option::is_some)
        && constant_scale.as_ref().is_none_or(Option::is_some)
    {
        return Ok(NetImmerseObjectTransformMotion::ConstantPose {
            translation_m: constant_translation
                .flatten()
                .map(|values| [values[0], values[1], values[2]]),
            rotation_xyzw: constant_rotation.flatten(),
            uniform_scale: constant_scale.flatten(),
        });
    }
    Ok(NetImmerseObjectTransformMotion::Animation(Box::new(
        PrefabTransformAnimation {
            clock: PrefabControllerClock {
                flags: controller.controller.flags,
                frequency: controller.controller.frequency,
                phase: controller.controller.phase,
                start_time_s: controller.controller.start_time,
                stop_time_s: controller.controller.stop_time,
            },
            translation_m,
            rotation,
            uniform_scale,
        },
    )))
}

fn float_key_group_from_vector3_component(
    group: &NetImmerseVector3KeyGroup,
    axis: usize,
) -> NetImmerseFloatKeyGroup {
    NetImmerseFloatKeyGroup {
        interpolation: group.interpolation,
        keys: group
            .keys
            .iter()
            .map(|key| NetImmerseFloatKey {
                time: key.time,
                value: key.value[axis],
                forward: key.forward.map(|forward| forward[axis]),
                backward: key.backward.map(|backward| backward[axis]),
                tbc: key.tbc,
            })
            .collect(),
    }
}

fn lower_netimmerse_float_key_group_to_prefab_curve(
    source_path: &str,
    group: &NetImmerseFloatKeyGroup,
) -> Result<PrefabScalarCurve> {
    let segments = lower_scalar_track(group).map_err(|error| {
        NativeSceneLoweringError::new(
            source_path,
            format!("invalid NIF keyframe track: {error:#}"),
        )
    })?;
    Ok(PrefabScalarCurve {
        segments: segments
            .iter()
            .map(|(start_time_s, segment)| PrefabScalarCurveSegment {
                start_time_s: *start_time_s,
                coefficients: segment.coeff,
            })
            .collect(),
    })
}

fn negate_prefab_curve(mut curve: PrefabScalarCurve) -> PrefabScalarCurve {
    for segment in &mut curve.segments {
        segment.coefficients = segment.coefficients.map(|coefficient| -coefficient);
    }
    curve
}

fn constant_prefab_curve_value(curve: &PrefabScalarCurve) -> Option<f32> {
    let first = curve.segments.first()?.coefficients[0];
    curve
        .segments
        .iter()
        .all(|segment| {
            let [constant, linear, quadratic, cubic] = segment.coefficients;
            constant.to_bits() == first.to_bits()
                && linear == 0.0
                && quadratic == 0.0
                && cubic == 0.0
        })
        .then_some(first)
}

fn constant_prefab_rotation(rotation: &PrefabRotationCurve) -> Option<[f32; 4]> {
    match rotation {
        PrefabRotationCurve::EulerAnglesXzy(curves) => {
            let [x, z, y] = [
                constant_prefab_curve_value(&curves[0])?,
                constant_prefab_curve_value(&curves[1])?,
                constant_prefab_curve_value(&curves[2])?,
            ];
            Some(
                (Quat::from_rotation_x(x) * Quat::from_rotation_z(z) * Quat::from_rotation_y(y))
                    .to_array(),
            )
        }
        PrefabRotationCurve::Quaternions { keys, .. } => {
            let first = keys.first()?.rotation_xyzw;
            keys.iter()
                .all(|key| key.rotation_xyzw.map(f32::to_bits) == first.map(f32::to_bits))
                .then_some(first)
        }
    }
}

fn lower_netimmerse_quaternion_keys(
    source_path: &str,
    rotation_type: u32,
    keys: &[NetImmerseQuaternionKey],
) -> Result<PrefabRotationCurve> {
    // Source keys are stored W, X, Y, Z. Their inverse, converted from source
    // [X, Y, Z] to Bevy [X, Z, -Y], matches authored node matrices.
    let convert = |time_s: f32, [w, x, y, z]: [f32; 4]| PrefabRotationKey {
        time_s,
        rotation_xyzw: [-x, -z, y, w],
    };
    let (keys, stepped) = match rotation_type {
        1 | 5 => (
            keys.iter()
                .map(|key| convert(key.time, key.value))
                .collect::<Vec<_>>(),
            rotation_type == 5,
        ),
        3 => {
            let sampled = sample_netimmerse_keyframes(
                Some(3),
                &keys
                    .iter()
                    .map(|key| NetImmerseSourceKeyframe {
                        time_seconds: key.time,
                        value: key.value,
                        outgoing_bezier_control_value: None,
                        incoming_bezier_control_value: None,
                        tension_bias_continuity: key.tbc,
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| {
                NativeSceneLoweringError::new(
                    source_path,
                    format!("invalid NIF TCB rotation keys: {error}"),
                )
            })?;
            (
                sampled
                    .into_iter()
                    .map(|(time, value)| convert(time, normalize_wxyz_quaternion(value)))
                    .collect(),
                false,
            )
        }
        other => {
            return Err(NativeSceneLoweringError::new(
                source_path,
                format!("uses unsupported NIF rotation key type {other}"),
            ));
        }
    };
    if keys.iter().any(|key| {
        !key.time_s.is_finite()
            || key
                .rotation_xyzw
                .iter()
                .any(|component| !component.is_finite())
    }) || keys.windows(2).any(|pair| pair[1].time_s < pair[0].time_s)
    {
        return Err(NativeSceneLoweringError::new(
            source_path,
            "contains non-finite or unordered NIF rotation keys",
        ));
    }
    Ok(PrefabRotationCurve::Quaternions { keys, stepped })
}

fn normalize_wxyz_quaternion(quaternion: [f32; 4]) -> [f32; 4] {
    let length = quaternion
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if length > f32::EPSILON {
        quaternion.map(|value| value / length)
    } else {
        [1.0, 0.0, 0.0, 0.0]
    }
}
