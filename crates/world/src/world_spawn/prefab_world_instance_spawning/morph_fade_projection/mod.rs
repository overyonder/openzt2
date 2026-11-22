use std::time::Duration;

use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateSet},
    color::Alpha,
    prelude::*,
};
use openzt2_world_state::application_schedule::FixedGameSet;

/// The opacity interval of the old or replacement prefab during a morph.
#[derive(Component)]
pub struct PrefabMorphFade {
    timer: Timer,
    fades_in: bool,
}

impl PrefabMorphFade {
    pub fn fade_in(duration: Duration) -> Self {
        Self {
            timer: Timer::new(duration, TimerMode::Once),
            fades_in: true,
        }
    }

    pub fn fade_out(duration: Duration) -> Self {
        Self {
            timer: Timer::new(duration, TimerMode::Once),
            fades_in: false,
        }
    }

    pub fn opacity(&self) -> f32 {
        if self.fades_in {
            self.timer.fraction()
        } else {
            1.0 - self.timer.fraction()
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct PrefabMorphOpacity(pub(crate) f32);

#[derive(Component)]
struct PrefabMorphStandardMaterial {
    source: Handle<StandardMaterial>,
    variant: Handle<StandardMaterial>,
}

pub(super) fn register_prefab_morph_fades(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<PrefabMorphOpacity>::new(
        PostUpdate,
    ))
    .add_systems(
        FixedUpdate,
        advance_morph_fade_timers.in_set(FixedGameSet::Act),
    )
    .add_systems(
        PostUpdate,
        (
            publish_prefab_morph_opacity.before(PropagateSet::<PrefabMorphOpacity>::default()),
            project_morph_opacity_onto_standard_materials
                .after(PropagateSet::<PrefabMorphOpacity>::default()),
        ),
    );
}

fn advance_morph_fade_timers(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut fades: Query<(Entity, &mut PrefabMorphFade)>,
) {
    for (entity, mut fade) in &mut fades {
        if !fade.timer.is_finished() {
            fade.timer.tick(time.delta());
        }
        if fade.timer.is_finished() && fade.fades_in {
            commands.entity(entity).remove::<PrefabMorphFade>();
        }
    }
}

fn publish_prefab_morph_opacity(
    mut commands: Commands,
    mut changed: Query<
        (
            Entity,
            &PrefabMorphFade,
            Option<&mut Propagate<PrefabMorphOpacity>>,
        ),
        Changed<PrefabMorphFade>,
    >,
    mut removed: RemovedComponents<PrefabMorphFade>,
    live: Query<(), With<PrefabMorphFade>>,
) {
    for (entity, fade, inherited) in &mut changed {
        let opacity = Propagate(PrefabMorphOpacity(fade.opacity()));
        if let Some(mut inherited) = inherited {
            inherited.set_if_neq(opacity);
        } else {
            commands.entity(entity).try_insert(opacity);
        }
    }
    for entity in removed.read() {
        if !live.contains(entity) {
            commands
                .entity(entity)
                .try_remove::<Propagate<PrefabMorphOpacity>>();
        }
    }
}

#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
struct StandardMorphMaterial {
    entity: Entity,
    material: &'static mut MeshMaterial3d<StandardMaterial>,
    opacity: Option<&'static PrefabMorphOpacity>,
    binding: Option<&'static mut PrefabMorphStandardMaterial>,
}

fn project_morph_opacity_onto_standard_materials(
    mut commands: Commands,
    mut meshes: Query<
        StandardMorphMaterial,
        Or<(With<PrefabMorphOpacity>, With<PrefabMorphStandardMaterial>)>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut mesh in &mut meshes {
        let Some(opacity) = mesh.opacity else {
            if let Some(binding) = mesh.binding {
                if mesh.material.0 == binding.variant {
                    mesh.material.0 = binding.source.clone();
                }
                commands
                    .entity(mesh.entity)
                    .try_remove::<PrefabMorphStandardMaterial>();
            }
            continue;
        };

        if let Some(binding) = mesh
            .binding
            .as_ref()
            .filter(|binding| mesh.material.0 == binding.variant)
        {
            let Some(source) = materials.get(&binding.source) else {
                continue;
            };
            let alpha = source.base_color.alpha() * opacity.0;
            let changed = materials
                .get(&binding.variant)
                .is_some_and(|variant| variant.base_color.alpha().to_bits() != alpha.to_bits());
            if !changed {
                continue;
            }
            if let Some(variant) = materials.get_mut(&binding.variant) {
                variant.into_inner().base_color.set_alpha(alpha);
            }
            continue;
        }

        let Some(mut variant) = materials.get(&mesh.material.0).cloned() else {
            continue;
        };
        variant.alpha_mode = AlphaMode::Blend;
        variant
            .base_color
            .set_alpha(variant.base_color.alpha() * opacity.0);
        let binding = PrefabMorphStandardMaterial {
            source: mesh.material.0.clone(),
            variant: materials.add(variant),
        };
        mesh.material.0 = binding.variant.clone();
        if let Some(mut current) = mesh.binding {
            *current = binding;
        } else {
            commands.entity(mesh.entity).try_insert(binding);
        }
    }
}
