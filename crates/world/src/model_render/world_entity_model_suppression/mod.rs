use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::VisibilitySystems;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::{
    world_entity_crating::WorldEntityIsCrated, world_entity_view_filter::HiddenByWorldViewFilter,
};
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct ModelRenderingSuppressed;

#[derive(Component)]
#[require(Transform, Visibility)]
pub(crate) struct ModelRenderVisibilityGate;

pub(super) fn register_inherited_model_render_suppression(app: &mut App) {
    app.add_plugins(HierarchyPropagatePlugin::<ModelRenderingSuppressed>::new(
        PostUpdate,
    ))
    .add_systems(
        PostUpdate,
        (
            publish_model_rendering_suppression_sources
                .before(PropagateSet::<ModelRenderingSuppressed>::default()),
            apply_model_render_visibility_gates
                .after(PropagateSet::<ModelRenderingSuppressed>::default())
                .before(VisibilitySystems::VisibilityPropagate),
        ),
    );
}

fn publish_model_rendering_suppression_sources(
    mut commands: Commands,
    added: Query<
        Entity,
        Or<(
            Added<RelocatingPlacedObject>,
            Added<WorldEntityIsCrated>,
            Added<HiddenByWorldViewFilter>,
        )>,
    >,
    mut relocation_ended: RemovedComponents<RelocatingPlacedObject>,
    mut uncrated: RemovedComponents<WorldEntityIsCrated>,
    mut filter_released: RemovedComponents<HiddenByWorldViewFilter>,
    sources: Query<(
        Has<RelocatingPlacedObject>,
        Has<WorldEntityIsCrated>,
        Has<HiddenByWorldViewFilter>,
        Has<Propagate<ModelRenderingSuppressed>>,
    )>,
    mut changed: Local<EntityHashSet>,
) {
    changed.clear();
    changed.extend(
        added
            .iter()
            .chain(relocation_ended.read())
            .chain(uncrated.read())
            .chain(filter_released.read()),
    );
    for &entity in changed.iter() {
        let Ok((relocating, crated, filtered, propagated)) = sources.get(entity) else {
            continue;
        };
        match (relocating || crated || filtered, propagated) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert(Propagate(ModelRenderingSuppressed));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .remove::<Propagate<ModelRenderingSuppressed>>();
            }
            _ => {}
        }
    }
}

fn apply_model_render_visibility_gates(
    changed: Query<
        Entity,
        (
            With<ModelRenderVisibilityGate>,
            Or<(
                Changed<ModelRenderingSuppressed>,
                Added<ModelRenderVisibilityGate>,
            )>,
        ),
    >,
    mut removed: RemovedComponents<ModelRenderingSuppressed>,
    mut gates: Query<
        (&mut Visibility, Has<ModelRenderingSuppressed>),
        With<ModelRenderVisibilityGate>,
    >,
) {
    for entity in changed.iter().chain(removed.read()) {
        if let Ok((mut visibility, suppressed)) = gates.get_mut(entity) {
            visibility.set_if_neq(if suppressed {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::visibility::RenderLayers;

    #[test]
    fn overlapping_holds_preserve_authored_visibility_and_render_layers() {
        let mut app = App::new();
        register_inherited_model_render_suppression(&mut app);
        let animal = app
            .world_mut()
            .spawn((
                WorldEntityIsCrated,
                HiddenByWorldViewFilter,
                Visibility::Hidden,
            ))
            .id();
        let gate = app
            .world_mut()
            .spawn((ModelRenderVisibilityGate, ChildOf(animal)))
            .id();
        let mesh = app
            .world_mut()
            .spawn((Mesh3d::default(), RenderLayers::layer(3), ChildOf(gate)))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<Visibility>(gate),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .entity_mut(animal)
            .remove::<HiddenByWorldViewFilter>();
        app.world_mut()
            .entity_mut(mesh)
            .insert(RenderLayers::layer(5));
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(gate),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .entity_mut(animal)
            .remove::<WorldEntityIsCrated>();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(gate),
            Some(&Visibility::Inherited)
        );
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<RenderLayers>(mesh),
            Some(&RenderLayers::layer(5))
        );
    }
}
