use std::time::Duration;

use bevy::{color::Alpha, platform::collections::HashSet, prelude::*};

/// The opacity interval of the old or replacement prefab during a morph.
#[derive(Component)]
pub struct PrefabMorphFade {
    timer: Timer,
    from: f32,
    to: f32,
}

impl PrefabMorphFade {
    pub fn fade_in(duration: Duration) -> Self {
        Self {
            timer: Timer::new(duration, TimerMode::Once),
            from: 0.0,
            to: 1.0,
        }
    }

    pub fn fade_out(duration: Duration) -> Self {
        Self {
            timer: Timer::new(duration, TimerMode::Once),
            from: 1.0,
            to: 0.0,
        }
    }

    pub fn opacity(&self) -> f32 {
        (self.to - self.from).mul_add(self.timer.fraction(), self.from)
    }
}

#[derive(Component)]
pub(crate) struct PrefabMorphStandardMaterial {
    source: Handle<StandardMaterial>,
    variant: Handle<StandardMaterial>,
}

pub(crate) fn advance_morph_fade_timers(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut fades: Query<(Entity, &mut PrefabMorphFade)>,
) {
    for (entity, mut fade) in &mut fades {
        if !fade.timer.is_finished() {
            fade.timer.tick(time.delta());
        }
        if fade.timer.is_finished() && fade.to == 1.0 {
            commands.entity(entity).remove::<PrefabMorphFade>();
        }
    }
}

pub(crate) fn project_morph_fade_onto_prefab_materials(
    mut commands: Commands,
    fades: Query<(Entity, &PrefabMorphFade)>,
    mut removed: RemovedComponents<PrefabMorphFade>,
    children: Query<&Children>,
    mut meshes: Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&mut PrefabMorphStandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut faded_meshes = HashSet::<Entity>::default();
    for (root, fade) in &fades {
        let opacity = fade.opacity();
        for entity in std::iter::once(root).chain(children.iter_descendants::<Children>(root)) {
            let Ok((mut material, binding)) = meshes.get_mut(entity) else {
                continue;
            };
            if !faded_meshes.insert(entity) {
                continue;
            }
            if let Some(mut current) = binding {
                if material.0 == current.variant {
                    if let Some(source_alpha) = materials
                        .get(&current.source)
                        .map(|source| source.base_color.alpha())
                    {
                        if let Some(mut variant) = materials.get_mut(&current.variant) {
                            if variant.base_color.alpha() != source_alpha * opacity {
                                variant.base_color.set_alpha(source_alpha * opacity);
                            }
                        }
                    }
                } else if let Some(mut variant) = materials.get(&material.0).cloned() {
                    variant.alpha_mode = AlphaMode::Blend;
                    variant
                        .base_color
                        .set_alpha(variant.base_color.alpha() * opacity);
                    current.source = material.0.clone();
                    current.variant = materials.add(variant);
                    material.0 = current.variant.clone();
                }
            } else if let Some(mut variant) = materials.get(&material.0).cloned() {
                variant.alpha_mode = AlphaMode::Blend;
                variant
                    .base_color
                    .set_alpha(variant.base_color.alpha() * opacity);
                let source = material.0.clone();
                let variant = materials.add(variant);
                material.0 = variant.clone();
                commands
                    .entity(entity)
                    .try_insert(PrefabMorphStandardMaterial { source, variant });
            }
        }
    }
    for root in removed.read() {
        for entity in std::iter::once(root).chain(children.iter_descendants::<Children>(root)) {
            if faded_meshes.contains(&entity) {
                continue;
            }
            let Ok((mut material, Some(binding))) = meshes.get_mut(entity) else {
                continue;
            };
            if material.0 == binding.variant {
                material.0 = binding.source.clone();
            }
            commands
                .entity(entity)
                .try_remove::<PrefabMorphStandardMaterial>();
        }
    }
}
