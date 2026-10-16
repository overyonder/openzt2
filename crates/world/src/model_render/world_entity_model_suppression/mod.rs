use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::{
    world_entity_crating::WorldEntityIsCrated, world_entity_view_filter::HiddenByWorldViewFilter,
};
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

pub(super) fn project_world_entity_model_suppression(
    mut commands: Commands,
    suppression_sources: Query<
        Entity,
        Or<(
            With<RelocatingPlacedObject>,
            With<WorldEntityIsCrated>,
            With<HiddenByWorldViewFilter>,
        )>,
    >,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    suppressed: Query<Entity, With<SuppressedModelRenderLayers>>,
    mut meshes: Query<
        (
            Entity,
            Option<&RenderLayers>,
            Option<&mut SuppressedModelRenderLayers>,
        ),
        Or<(With<Mesh3d>, With<PointLight>, With<SpotLight>, With<DirectionalLight>, With<openzt2_world_state::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution>)>,
    >,
) {
    // Restore only our temporary override, including the absence of an explicit
    // layer policy. An authored hidden model is never made visible here.
    for entity in &suppressed {
        let Ok((_, layers, saved)) = meshes.get_mut(entity) else {
            continue;
        };
        let Some(saved) = saved else { continue };
        if suppression_sources.contains(entity)
            || parents
                .iter_ancestors(entity)
                .any(|parent| suppression_sources.contains(parent))
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
        target.remove::<SuppressedModelRenderLayers>();
    }
    for source in &suppression_sources {
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
                    .insert(SuppressedModelRenderLayers(layers.cloned()));
            }
            commands.entity(entity).insert(RenderLayers::none());
        }
    }
}

/// The model's render policy underneath temporary view, crate or relocation suppression.
/// Visibility and loaded model assets remain owned by their ordinary systems.
#[derive(Component)]
pub(crate) struct SuppressedModelRenderLayers(pub(crate) Option<RenderLayers>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_view_filter_does_not_reveal_a_crated_model_or_change_authored_visibility() {
        let mut app = App::new();
        app.add_systems(Update, project_world_entity_model_suppression);
        let animal = app
            .world_mut()
            .spawn((
                WorldEntityIsCrated,
                HiddenByWorldViewFilter,
                Visibility::Hidden,
            ))
            .id();
        let mesh = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                RenderLayers::layer(3),
                Visibility::Inherited,
                ChildOf(animal),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(mesh),
            Some(&RenderLayers::none())
        );
        app.world_mut()
            .entity_mut(animal)
            .remove::<HiddenByWorldViewFilter>();
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(mesh),
            Some(&RenderLayers::none())
        );
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .entity_mut(animal)
            .remove::<WorldEntityIsCrated>();
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(mesh),
            Some(&RenderLayers::layer(3))
        );
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Hidden)
        );
    }
}
