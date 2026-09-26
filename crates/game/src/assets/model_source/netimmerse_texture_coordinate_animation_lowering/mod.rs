//! Lower native UV keys once into Bevy polynomial segments for material uniforms.

#[cfg(test)]
mod tests;

use anyhow::{bail, ensure, Context, Result};
use bevy::math::cubic_splines::CubicSegment;
use std::{collections::BTreeSet, sync::Arc};

use super::netimmerse_nif_source::{
    block_payload::NetImmerseNifBlockPayload, document_source_types::NetImmerseNifDocument,
    interpolated_key_source_types::NetImmerseFloatKeyGroup,
};
use crate::assets::material::runtime::texture_coordinate_animation::TextureCoordinateAnimation;

pub(super) fn lower_texture_coordinate_animations(
    document: &NetImmerseNifDocument,
    geometry_index: u32,
) -> Result<Arc<[TextureCoordinateAnimation]>> {
    let geometry_reference = i32::try_from(geometry_index)?;
    let block = document
        .block(geometry_reference)
        .context("missing UV animation geometry")?;
    let object = block
        .payload
        .av_object()
        .context("UV animation target is not a scene object")?;
    let geometry_uv_set_count = geometry_uv_set_count(document, &block.payload)?;
    let mut reference = object.object.controller_ref;
    let mut visited = BTreeSet::new();
    let mut animations = Vec::new();
    while reference >= 0 {
        ensure!(
            visited.insert(reference),
            "cyclic NIF controller chain at {reference}"
        );
        let payload = &document
            .block(reference)
            .context("missing NIF controller")?
            .payload;
        match payload {
            NetImmerseNifBlockPayload::NiUVController(uv) => {
                let controller = &uv.controller;
                ensure!(
                    controller.target_ref == geometry_reference,
                    "UV controller target differs from its geometry owner"
                );
                // The native controller only rewrites a texture set that the
                // geometry data owns. A higher index, including the 0xFFFF
                // written for untextured geometry, never changes anything.
                if usize::from(uv.texture_set) >= geometry_uv_set_count {
                    reference = controller.next_controller_ref;
                    continue;
                }
                ensure!(
                    uv.texture_set < 3,
                    "UV controller requires unsupported mesh UV set {}",
                    uv.texture_set
                );
                ensure!(
                    [
                        controller.frequency,
                        controller.phase,
                        controller.start_time,
                        controller.stop_time
                    ]
                    .into_iter()
                    .all(f32::is_finite)
                        && controller.stop_time >= controller.start_time,
                    "invalid UV controller timing"
                );
                let data = &document
                    .block(uv.data_ref)
                    .context("missing NiUVData")?
                    .payload;
                let NetImmerseNifBlockPayload::NiUVData(data) = data else {
                    bail!("UV controller does not reference NiUVData");
                };
                ensure!(data.groups.len() == 4, "NiUVData must contain four tracks");
                let tracks = data
                    .groups
                    .iter()
                    .map(lower_scalar_track)
                    .collect::<Result<Vec<_>>>()?;
                animations.push(TextureCoordinateAnimation {
                    uv_set: uv.texture_set,
                    flags: controller.flags,
                    frequency: controller.frequency,
                    phase: controller.phase,
                    interval: controller.start_time..=controller.stop_time,
                    tracks: tracks
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("NiUVData track count changed"))?,
                });
                reference = controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiKeyframeController(value) => {
                reference = value.controller.next_controller_ref;
            }
            NetImmerseNifBlockPayload::NiVisController(value) => {
                reference = value.controller.next_controller_ref;
            }
            // Other controller semantics remain checked by scene lowering.
            _ => break,
        }
    }
    Ok(animations.into())
}

fn geometry_uv_set_count(
    document: &NetImmerseNifDocument,
    geometry: &NetImmerseNifBlockPayload,
) -> Result<usize> {
    let data_reference = match geometry {
        NetImmerseNifBlockPayload::NiTriShape(shape) => shape.geometry.data_ref,
        NetImmerseNifBlockPayload::NiTriStrips(strips) => strips.geometry.data_ref,
        _ => bail!("UV animation target is not triangle geometry"),
    };
    match &document
        .block(data_reference)
        .context("missing UV animation geometry data")?
        .payload
    {
        NetImmerseNifBlockPayload::NiTriShapeData(data) => Ok(data.geometry.uv_sets.len()),
        NetImmerseNifBlockPayload::NiTriStripsData(data) => Ok(data.geometry.uv_sets.len()),
        _ => bail!("UV animation geometry does not reference triangle data"),
    }
}

/// Lowers one NetImmerse float key group into Hermite polynomial segments.
pub(super) fn lower_scalar_track(
    group: &NetImmerseFloatKeyGroup,
) -> Result<Box<[(f32, CubicSegment<f32>)]>> {
    ensure!(
        group
            .keys
            .iter()
            .all(|key| key.time.is_finite() && key.value.is_finite()),
        "non-finite key"
    );
    ensure!(
        group
            .keys
            .windows(2)
            .all(|keys| keys[1].time > keys[0].time),
        "key times must increase"
    );
    let mut segments = Vec::with_capacity(group.keys.len());
    for (index, key) in group.keys.iter().enumerate() {
        let Some(next) = group.keys.get(index + 1) else {
            segments.push((
                key.time,
                CubicSegment {
                    coeff: [key.value, 0.0, 0.0, 0.0],
                },
            ));
            break;
        };
        let difference = next.value - key.value;
        let coeff = match group.interpolation {
            Some(1) => [key.value, difference, 0.0, 0.0],
            Some(5) => [key.value, 0.0, 0.0, 0.0],
            Some(2 | 3) => {
                let (outgoing, incoming) = if group.interpolation == Some(2) {
                    // Tangents are stored incoming first, then outgoing.
                    (
                        key.backward.context("missing outgoing tangent")?,
                        next.forward.context("missing incoming tangent")?,
                    )
                } else {
                    (
                        tcb_tangents(group, index)?.1,
                        tcb_tangents(group, index + 1)?.0,
                    )
                };
                [
                    key.value,
                    outgoing,
                    2.0_f32.mul_add(-outgoing, 3.0 * difference) - incoming,
                    (-2.0_f32).mul_add(difference, outgoing) + incoming,
                ]
            }
            kind => bail!("unsupported key interpolation {kind:?}"),
        };
        ensure!(
            coeff.into_iter().all(f32::is_finite),
            "non-finite key polynomial"
        );
        segments.push((key.time, CubicSegment { coeff }));
    }
    Ok(segments.into_boxed_slice())
}

fn tcb_tangents(group: &NetImmerseFloatKeyGroup, index: usize) -> Result<(f32, f32)> {
    let key = &group.keys[index];
    let [tension, bias, continuity] = key.tbc.context("missing TCB coefficients")?;
    let previous = index.checked_sub(1).and_then(|index| group.keys.get(index));
    let next = group.keys.get(index + 1);
    let before = previous.map_or_else(
        || next.map_or(0.0, |next| next.value - key.value),
        |previous| key.value - previous.value,
    );
    let after = next.map_or(before, |next| next.value - key.value);
    let (previous_interval, next_interval) = match (previous, next) {
        (Some(previous), Some(next)) => (key.time - previous.time, next.time - key.time),
        _ => (1.0, 1.0),
    };
    let normalization = (1.0 - tension) / (previous_interval + next_interval);
    Ok((
        normalization
            * previous_interval
            * ((1.0 + bias) * (1.0 - continuity))
                .mul_add(after, (1.0 - bias) * (1.0 + continuity) * before),
        normalization
            * next_interval
            * ((1.0 - continuity) * (1.0 - bias))
                .mul_add(after, (1.0 + continuity) * (1.0 + bias) * before),
    ))
}
