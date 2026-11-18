use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::prelude::*;
use bevy_hanabi::{EffectProperties, EffectSystems, ParticleEffect};

/// World-space plane coefficients, shared by an effect's plane modifiers.
#[derive(Component, Clone, Copy)]
pub struct ParticleEffectPlane(pub Vec4);

#[derive(Component, Clone, Copy, PartialEq)]
struct InheritedParticleEffectPlane(Vec4);

#[derive(Component)]
#[require(EffectProperties)]
pub(super) struct UsesParticleEffectPlane;

pub(super) struct ParticleEffectPlaneBindingPlugin;

impl Plugin for ParticleEffectPlaneBindingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HierarchyPropagatePlugin::<InheritedParticleEffectPlane>::new(PostUpdate))
            .add_systems(
                PostUpdate,
                (
                    project_particle_effect_plane_sources
                        .before(PropagateSet::<InheritedParticleEffectPlane>::default()),
                    upload_inherited_particle_effect_planes
                        .after(PropagateSet::<InheritedParticleEffectPlane>::default())
                        .before(EffectSystems::UpdatePropertiesFromAsset),
                ),
            );
    }
}

fn project_particle_effect_plane_sources(
    mut commands: Commands,
    changed_planes: Query<(Entity, &ParticleEffectPlane), Changed<ParticleEffectPlane>>,
    mut removed_planes: RemovedComponents<ParticleEffectPlane>,
    mut propagation: Query<&mut Propagate<InheritedParticleEffectPlane>>,
) {
    for (entity, plane) in &changed_planes {
        let inherited = InheritedParticleEffectPlane(plane.0);
        if let Ok(mut source) = propagation.get_mut(entity) {
            source.set_if_neq(Propagate(inherited));
        } else {
            commands.entity(entity).insert(Propagate(inherited));
        }
    }
    for entity in removed_planes.read() {
        if !changed_planes.contains(entity) {
            if let Ok(mut entity_commands) = commands.get_entity(entity) {
                entity_commands.remove::<Propagate<InheritedParticleEffectPlane>>();
            }
        }
    }
}

fn upload_inherited_particle_effect_planes(
    changed: Query<
        Entity,
        (
            With<UsesParticleEffectPlane>,
            Or<(
                Changed<ParticleEffect>,
                Changed<InheritedParticleEffectPlane>,
                Added<UsesParticleEffectPlane>,
            )>,
        ),
    >,
    mut removed_planes: RemovedComponents<InheritedParticleEffectPlane>,
    mut effects: Query<
        (Option<&InheritedParticleEffectPlane>, &mut EffectProperties),
        (With<ParticleEffect>, With<UsesParticleEffectPlane>),
    >,
) {
    for entity in changed.iter().chain(removed_planes.read()) {
        let Ok((plane, properties)) = effects.get_mut(entity) else {
            continue;
        };
        let coefficients = plane.map_or(Vec4::ZERO, |plane| plane.0);
        let _ = EffectProperties::set_if_changed(
            properties,
            "particle_plane_override",
            coefficients.into(),
        );
    }
}
