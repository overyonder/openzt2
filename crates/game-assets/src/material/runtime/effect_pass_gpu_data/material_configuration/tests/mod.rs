use bevy::render::render_resource::BufferUsages;

use super::*;

#[test]
fn scoped_lighting_binds_a_storage_buffer() {
    let mut buffers = Assets::<ShaderBuffer>::default();
    let mut bindings = EffectPassTextureAssets::from_ordered_texture_assets(Default::default());
    let lights = scoped_lighting_context(
        &[D3d9ScopedPrefabLight::Ambient { color: Vec3::ONE }],
        Vec3::ZERO,
    );

    bindings.bind_scoped_lights(lights, &mut buffers);

    let buffer = buffers.get(&bindings.fixed_function_world_lights).unwrap();
    assert!(buffer.buffer_usage.contains(BufferUsages::STORAGE));
    assert!(!buffer.use_uniform_buffer_pages);
    let bytes = buffer.cast_slice::<u8>().unwrap();
    let ambient_register = [1.0_f32, 1.0, 1.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(&bytes[..16], ambient_register);
}
