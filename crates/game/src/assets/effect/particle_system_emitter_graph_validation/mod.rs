//! Validates authored collision-spawn emitter references and acyclic ownership.

use std::collections::HashMap;

use openzt2_game_data::{
    particle::authored_particle_system::{
        AuthoredParticleSystemEmitter as EffectEmitter,
        AuthoredParticleSystemModifier as EffectModifier,
    },
    AssetId,
};

use super::particle_system_source_conversion_error::{
    particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
};

pub(super) fn validate_particle_system_collision_spawn_emitter_graph(
    particle_system_asset_path: &str,
    effect_emitters: &[EffectEmitter],
) -> Result<(), ParticleSystemSourceConversionError> {
    let effect_emitters_by_id = effect_emitters
        .iter()
        .map(|effect_emitter| (effect_emitter.emitter_id, effect_emitter))
        .collect::<HashMap<_, _>>();
    let mut emitter_visit_state_by_id = HashMap::<AssetId, u8>::new();
    for effect_emitter in effect_emitters {
        validate_particle_system_collision_spawn_emitter_descendants(
            particle_system_asset_path,
            effect_emitter.emitter_id,
            &effect_emitters_by_id,
            &mut emitter_visit_state_by_id,
        )?;
    }
    Ok(())
}

fn validate_particle_system_collision_spawn_emitter_descendants(
    particle_system_asset_path: &str,
    emitter_id: AssetId,
    effect_emitters_by_id: &HashMap<AssetId, &EffectEmitter>,
    emitter_visit_state_by_id: &mut HashMap<AssetId, u8>,
) -> Result<(), ParticleSystemSourceConversionError> {
    match emitter_visit_state_by_id.get(&emitter_id).copied() {
        Some(1) => {
            return Err(particle_system_source_conversion_failure(
                particle_system_asset_path,
                emitter_id.to_lowercase_hexadecimal_string(),
                "particle collision-spawn graph is cyclic",
            ));
        }
        Some(2) => return Ok(()),
        _ => {}
    }
    let effect_emitter = effect_emitters_by_id
        .get(&emitter_id)
        .copied()
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                particle_system_asset_path,
                emitter_id.to_lowercase_hexadecimal_string(),
                "particle collision-spawn target is missing",
            )
        })?;
    emitter_visit_state_by_id.insert(emitter_id, 1);
    for target_emitter_id in effect_emitter
        .modifiers
        .iter()
        .filter_map(|effect_modifier| match effect_modifier {
            EffectModifier::SpawnOnPlane { target_emitter, .. } => Some(*target_emitter),
            _ => None,
        })
    {
        validate_particle_system_collision_spawn_emitter_descendants(
            particle_system_asset_path,
            target_emitter_id,
            effect_emitters_by_id,
            emitter_visit_state_by_id,
        )?;
    }
    emitter_visit_state_by_id.insert(emitter_id, 2);
    Ok(())
}
