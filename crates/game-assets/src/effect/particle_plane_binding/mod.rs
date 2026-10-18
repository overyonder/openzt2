use bevy::prelude::*;
use bevy_hanabi::{EffectProperties, ParticleEffect};

/// World-space plane coefficients, shared by an effect's plane modifiers.
#[derive(Component, Clone, Copy)]
pub struct ParticleEffectPlane(pub Vec4);

pub fn upload_particle_effect_planes(
    mut commands: Commands,
    parents: Query<(Option<&ChildOf>, Option<&ParticleEffectPlane>)>,
    mut effects: Query<(Entity, Option<&mut EffectProperties>), With<ParticleEffect>>,
) {
    for (entity, properties) in &mut effects {
        let mut current = entity;
        let plane = loop {
            let Ok((parent, plane)) = parents.get(current) else {
                break None;
            };
            if let Some(plane) = plane {
                break Some(plane.0);
            }
            let Some(parent) = parent else { break None };
            current = parent.parent();
        };
        let Some(plane) = plane else { continue };
        if let Some(properties) = properties {
            let _ = EffectProperties::set_if_changed(
                properties,
                "particle_plane_override",
                plane.into(),
            );
        } else {
            let mut properties = EffectProperties::default();
            properties.set("particle_plane_override", plane.into());
            commands.entity(entity).insert(properties);
        }
    }
}
