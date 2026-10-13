use bevy::prelude::*;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::ActivateTerrainWaterImpactWave;

pub(super) fn apply_authored_water_impact_events_to_geometric_wave_channels(
    mut impacts: MessageReader<ActivateTerrainWaterImpactWave>,
    water_surfaces: Query<(
        &AuthoredEffectTechniquePassSubmissionOrder,
        &openzt2_zoo_state::terrain::terrain_water_renderer_types::TerrainWaterWaveStateKey,
    )>,
    mut targets: Option<
        ResMut<openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets>,
    >,
) {
    for impact in impacts.read() {
        if let Some((_, key)) = water_surfaces
            .iter()
            .find(|(order, _)| order.belongs_to_technique_invocation(impact.surface))
        {
            if let Some(targets) = targets.as_deref_mut() {
                targets.queue_wave_impact(*key, impact.source_position, impact.native_strength);
            }
        }
    }
}
