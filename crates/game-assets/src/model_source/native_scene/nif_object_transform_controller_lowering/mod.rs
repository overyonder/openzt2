//! NetImmerse object controller chains lowered into prefab transform motion.

use std::collections::BTreeSet;

use bevy::math::Quat;
use openzt2_game_data::scene_prefab::{
    PrefabControllerClock, PrefabRotationCurve, PrefabRotationCycle, PrefabRotationKey,
    PrefabScalarCurve, PrefabScalarCurveSegment, PrefabTransformAnimation,
    PrefabVisibilityAnimation,
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

#[derive(Default)]
pub(super) struct NetImmerseObjectTransformMotion {
    pub(super) translation_m: Option<[f32; 3]>,
    pub(super) rotation_xyzw: Option<[f32; 4]>,
    pub(super) uniform_scale: Option<f32>,
    pub(super) rotation_cycle: Option<PrefabRotationCycle>,
    pub(super) animations: Vec<PrefabTransformAnimation>,
    pub(super) visibility: Vec<(i32, PrefabVisibilityAnimation)>,
}

pub(super) fn lower_netimmerse_object_transform_motion(
    document: &NetImmerseNifDocument,
    object: &NetImmerseNiAvObject,
) -> Result<NetImmerseObjectTransformMotion> {
    let mut motion = NetImmerseObjectTransformMotion::default();
    let mut reference = object.object.controller_ref;
    let mut visited = BTreeSet::new();
    while reference >= 0 {
        if !visited.insert(reference) {
            return Err(NativeSceneLoweringError::new(
                &document.source_path,
                "contains a cyclic NIF object controller chain",
            ));
        }
        let block = document.block(reference).ok_or_else(|| {
            NativeSceneLoweringError::new(
                &document.source_path,
                format!("references missing NIF controller block {reference}"),
            )
        })?;
        reference = match &block.payload {
            NetImmerseNifBlockPayload::NiKeyframeController(keyframe) => {
                if keyframe.controller.flags & NETIMMERSE_CONTROLLER_ACTIVE_FLAG != 0 {
                    motion.apply_keyframe_controller(document, keyframe)?;
                }
                keyframe.controller.next_controller_ref
            }
            NetImmerseNifBlockPayload::NiUVController(uv) => {
                let is_geometry = document
                    .block(uv.controller.target_ref)
                    .is_some_and(|target| {
                        matches!(
                            target.payload,
                            NetImmerseNifBlockPayload::NiTriShape(_)
                                | NetImmerseNifBlockPayload::NiTriStrips(_)
                        )
                    });
                if !is_geometry {
                    return Err(NativeSceneLoweringError::new(
                        &document.source_path,
                        "UV controller target is not supported geometry",
                    ));
                }
                uv.controller.next_controller_ref
            }
            NetImmerseNifBlockPayload::NiVisController(visibility) => {
                let clock = &visibility.controller;
                if clock.is_active() && visibility.data_ref >= 0 {
                    let Some(NetImmerseNifBlockPayload::NiVisData(data)) = document
                        .block(visibility.data_ref)
                        .map(|block| &block.payload)
                    else {
                        return Err(NativeSceneLoweringError::new(
                            &document.source_path,
                            "visibility controller does not reference NiVisData",
                        ));
                    };
                    if data.keys.iter().any(|key| !key.time.is_finite())
                        || data.keys.windows(2).any(|keys| keys[1].time < keys[0].time)
                    {
                        return Err(NativeSceneLoweringError::new(
                            &document.source_path,
                            "invalid visibility key times",
                        ));
                    }
                    if !data.keys.is_empty() {
                        motion.visibility.push((
                            clock.target_ref,
                            PrefabVisibilityAnimation {
                                clock: clock.prefab_clock(),
                                keys: data
                                    .keys
                                    .iter()
                                    .map(|key| (key.time, key.value != 0))
                                    .collect(),
                            },
                        ));
                    }
                }
                clock.next_controller_ref
            }
            NetImmerseNifBlockPayload::NiGeomMorpherController(morph) => {
                morph.controller.next_controller_ref
            }
            NetImmerseNifBlockPayload::NiBoneLODController(lod) => {
                lod.controller.next_controller_ref
            }
            other => {
                return Err(NativeSceneLoweringError::new(
                    &document.source_path,
                    format!("uses unsupported object controller {other:?} at block {reference}"),
                ));
            }
        };
    }
    motion.animations.retain(|animation| {
        animation.translation_m.is_some()
            || animation.rotation.is_some()
            || animation.uniform_scale.is_some()
    });
    Ok(motion)
}

impl NetImmerseObjectTransformMotion {
    fn apply_keyframe_controller(
        &mut self,
        document: &NetImmerseNifDocument,
        keyframe: &NetImmerseNiKeyframeController,
    ) -> Result<()> {
        let Some(data_block) = document.block(keyframe.data_ref) else {
            if keyframe.data_ref < 0 {
                return Ok(());
            }
            return Err(NativeSceneLoweringError::new(
                &document.source_path,
                format!(
                    "references missing NIF keyframe data block {}",
                    keyframe.data_ref
                ),
            ));
        };
        let NetImmerseNifBlockPayload::NiKeyframeData(data) = &data_block.payload else {
            return Err(NativeSceneLoweringError::new(
                &document.source_path,
                format!(
                    "keyframe controller references {:?} instead of NiKeyframeData",
                    data_block.payload
                ),
            ));
        };
        let rotation_cycle = full_turn_xyz_rotation_cycle(keyframe, data);
        let mut animation =
            lower_netimmerse_keyframe_data_transform_motion(&document.source_path, keyframe, data)?;
        let replaces_translation = animation.translation_m.is_some();
        let replaces_rotation = animation.rotation.is_some();
        let replaces_scale = animation.uniform_scale.is_some();

        // The linked controllers write in order. Only a channel with keys
        // in a later controller supersedes a write to that channel.
        for earlier in &mut self.animations {
            if replaces_translation {
                earlier.translation_m = None;
            }
            if replaces_rotation {
                earlier.rotation = None;
            }
            if replaces_scale {
                earlier.uniform_scale = None;
            }
        }
        if replaces_rotation {
            if rotation_cycle.is_some() {
                animation.rotation = None;
            }
            self.rotation_cycle = rotation_cycle;
        }

        let fixed_translation = animation.translation_m.as_ref().map(|[x, y, z]| {
            Some([
                constant_prefab_curve_value(x)?,
                constant_prefab_curve_value(y)?,
                constant_prefab_curve_value(z)?,
            ])
        });
        let fixed_rotation = animation.rotation.as_ref().map(constant_prefab_rotation);
        let fixed_scale = animation
            .uniform_scale
            .as_ref()
            .map(constant_prefab_curve_value);
        if fixed_translation.as_ref().is_none_or(Option::is_some)
            && fixed_rotation.as_ref().is_none_or(Option::is_some)
            && fixed_scale.as_ref().is_none_or(Option::is_some)
        {
            self.translation_m = fixed_translation.flatten().or(self.translation_m);
            self.rotation_xyzw = fixed_rotation.flatten().or(self.rotation_xyzw);
            self.uniform_scale = fixed_scale.flatten().or(self.uniform_scale);
        } else if let Some(earlier) = self
            .animations
            .iter_mut()
            .find(|earlier| earlier.clock == animation.clock)
        {
            earlier.translation_m = animation
                .translation_m
                .or_else(|| earlier.translation_m.take());
            earlier.rotation = animation.rotation.or_else(|| earlier.rotation.take());
            earlier.uniform_scale = animation
                .uniform_scale
                .or_else(|| earlier.uniform_scale.take());
        } else {
            self.animations.push(animation);
        }
        Ok(())
    }
}

/// A single explicit revolution drives the cloud layer relative to its rest pose.
fn full_turn_xyz_rotation_cycle(
    controller: &NetImmerseNiKeyframeController,
    data: &NetImmerseNiKeyframeData,
) -> Option<PrefabRotationCycle> {
    if data.rotation_type != Some(NETIMMERSE_XYZ_ROTATION_KEY_TYPE)
        || data.xyz_rotations.len() != 3
        || data.xyz_rotations.iter().any(|axis| axis.keys.len() != 1)
    {
        return None;
    }
    let [x, y, z] = std::array::from_fn(|axis| data.xyz_rotations[axis].keys[0].value);
    let time = data.xyz_rotations[0].keys[0].time;
    let turns = [x, y, z].map(f32::abs);
    let clock = controller.controller.prefab_clock();
    if !time.is_finite()
        || time >= clock.stop_time_s
        || data
            .xyz_rotations
            .iter()
            .any(|axis| axis.keys[0].time != time)
        || turns
            .iter()
            .filter(|&&angle| (angle - std::f32::consts::TAU).abs() < 0.001)
            .count()
            != 1
        || turns.iter().filter(|&&angle| angle == 0.0).count() != 2
    {
        return None;
    }
    Some(PrefabRotationCycle {
        clock,
        key_time_s: time,
        angles_xzy: [-x, y, -z],
    })
}

fn lower_netimmerse_keyframe_data_transform_motion(
    source_path: &str,
    controller: &NetImmerseNiKeyframeController,
    data: &NetImmerseNiKeyframeData,
) -> Result<PrefabTransformAnimation> {
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
    Ok(PrefabTransformAnimation {
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
    })
}

pub(in crate::model_source) fn float_key_group_from_vector3_component(
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

pub(in crate::model_source) fn lower_netimmerse_float_key_group_to_prefab_curve(
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
