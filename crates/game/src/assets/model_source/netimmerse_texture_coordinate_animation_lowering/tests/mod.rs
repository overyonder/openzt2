use super::super::netimmerse_nif_source::interpolated_key_source_types::NetImmerseFloatKey;
use super::*;

fn key(time: f32, value: f32) -> NetImmerseFloatKey {
    NetImmerseFloatKey {
        time,
        value,
        forward: Some(0.0),
        backward: Some(0.0),
        tbc: Some([0.0; 3]),
    }
}

#[test]
fn cubic_uv_keys_are_hermite_tangents_not_bezier_positions() -> Result<()> {
    let keys = lower_scalar_track(&NetImmerseFloatKeyGroup {
        interpolation: Some(2),
        keys: vec![key(0.0, 0.0), key(1.0, 1.0)],
    })?;
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].1.position(0.25), 0.15625);
    assert_eq!(keys[0].1.position(1.0), 1.0);
    Ok(())
}

#[test]
fn tcb_endpoints_mirror_values_and_interior_uses_key_spacing() -> Result<()> {
    let group = NetImmerseFloatKeyGroup {
        interpolation: Some(3),
        keys: vec![key(0.0, 0.0), key(1.0, 2.0), key(4.0, 4.0)],
    };
    assert_eq!(tcb_tangents(&group, 0)?, (2.0, 2.0));
    assert_eq!(tcb_tangents(&group, 1)?, (1.0, 3.0));
    assert_eq!(tcb_tangents(&group, 2)?, (2.0, 2.0));
    Ok(())
}

#[test]
fn cubic_uv_tangent_order_matches_native_serialization() -> Result<()> {
    let mut first = key(0.0, 0.0);
    first.forward = Some(100.0);
    first.backward = Some(2.0);
    let mut last = key(1.0, 1.0);
    last.forward = Some(4.0);
    last.backward = Some(200.0);
    let segments = lower_scalar_track(&NetImmerseFloatKeyGroup {
        interpolation: Some(2),
        keys: vec![first, last],
    })?;
    assert_eq!(segments[0].1.velocity(0.0), 2.0);
    assert_eq!(segments[0].1.velocity(1.0), 4.0);
    Ok(())
}

#[test]
#[ignore = "requires the locally installed original Z2F"]
fn rainforest_lamp_uv_controller_reaches_animated_material() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("OPENZT2_Z2F_PATH")?);
    let archives = z2f::ArchiveSet::open(
        ["x300_000.z2f", "x301_000.z2f", "x302_000.z2f"].map(|name| directory.join(name)),
    )?;
    let path = "entities/objects/scenery/lamp_jt/lamp_jt.nif";
    let lowered = super::super::native_model_source_lowering::lower_native_model_source(
        path,
        &archives.read(std::path::Path::new(path))?,
        |reference| {
            archives
                .resolve_model_texture_reference(std::path::Path::new(path), reference)
                .map(|path| path.to_string_lossy().into_owned())
        },
    )?;
    assert!(lowered
        .materials
        .iter()
        .any(|material| !material.texture_coordinate_animations.is_empty()));
    assert!(lowered
        .materials
        .iter()
        .flat_map(|material| &material.evaluated_d3d9_effect.evaluated_techniques)
        .flat_map(|technique| &technique.evaluated_passes)
        .flat_map(|pass| &pass.evaluated_commands)
        .any(|command| matches!(
            command,
            d3d9_effects::effect_types::EvaluatedD3d9EffectCommand::D3d9TextureStageState {
                texture_stage_state: 17,
                state_value: 2,
                ..
            }
        )));
    Ok(())
}
