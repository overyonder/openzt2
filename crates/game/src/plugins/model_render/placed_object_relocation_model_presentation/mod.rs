use bevy::{camera::visibility::RenderLayers, prelude::*};

use crate::plugins::placement::placement_preview_types::RelocatingPlacedObject;

/// The model's render policy underneath a temporary relocation suppression.
/// Visibility and loaded model assets remain owned by their ordinary systems.
#[derive(Component)]
pub(super) struct RelocationModelRenderLayers(pub(super) Option<RenderLayers>);

pub(super) fn project_placed_object_relocation_model_presentation(
    mut commands: Commands,
    relocating: Query<Entity, With<RelocatingPlacedObject>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    suppressed: Query<Entity, With<RelocationModelRenderLayers>>,
    mut meshes: Query<
        (
            Entity,
            Option<&RenderLayers>,
            Option<&mut RelocationModelRenderLayers>,
        ),
        With<Mesh3d>,
    >,
) {
    // Restore only our temporary override, including the absence of an explicit
    // layer policy. An authored hidden model is never made visible here.
    for entity in &suppressed {
        let Ok((_, layers, saved)) = meshes.get_mut(entity) else {
            continue;
        };
        let Some(saved) = saved else { continue };
        if relocating.contains(entity)
            || parents
                .iter_ancestors(entity)
                .any(|parent| relocating.contains(parent))
        {
            continue;
        }
        let mut target = commands.entity(entity);
        if layers.is_some_and(|layers| *layers == RenderLayers::none()) {
            if let Some(layers) = &saved.0 {
                target.insert(layers.clone());
            } else {
                target.remove::<RenderLayers>();
            }
        }
        target.remove::<RelocationModelRenderLayers>();
    }
    for source in &relocating {
        for entity in
            std::iter::once(source).chain(children.iter_descendants_depth_first::<Children>(source))
        {
            let Ok((_, layers, saved)) = meshes.get_mut(entity) else {
                continue;
            };
            if layers.is_some_and(|layers| *layers == RenderLayers::none()) {
                continue;
            }
            // Material/view projection can update a mesh's policy while it is
            // moving. Keep its latest ordinary policy for restoration.
            if let Some(mut saved) = saved {
                saved.0 = layers.cloned();
            } else {
                commands
                    .entity(entity)
                    .insert(RelocationModelRenderLayers(layers.cloned()));
            }
            commands.entity(entity).insert(RenderLayers::none());
        }
    }
}
