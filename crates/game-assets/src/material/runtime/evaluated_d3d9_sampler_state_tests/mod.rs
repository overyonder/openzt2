use super::evaluated_d3d9_sampler_state::EvaluatedD3d9SamplerState;

#[test]
fn maps_evaluated_sampler_state_to_wgpu() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = EvaluatedD3d9SamplerState::default();
    for (state_code, value) in [(1, 3), (2, 2), (5, 2), (6, 3), (7, 2), (10, 8)] {
        state.apply(state_code, value)?;
    }
    state.validate()?;
    let descriptor = state.descriptor();
    assert_eq!(
        descriptor.address_mode_u,
        bevy::render::render_resource::AddressMode::ClampToEdge
    );
    assert_eq!(
        descriptor.address_mode_v,
        bevy::render::render_resource::AddressMode::MirrorRepeat
    );
    assert_eq!(
        descriptor.mag_filter,
        bevy::render::render_resource::FilterMode::Linear
    );
    assert_eq!(
        descriptor.min_filter,
        bevy::render::render_resource::FilterMode::Linear
    );
    assert_eq!(descriptor.anisotropy_clamp, 8);
    Ok(())
}
