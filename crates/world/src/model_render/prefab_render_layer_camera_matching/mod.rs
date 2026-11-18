use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::model_render::world_entity_model_suppression::SuppressedModelRenderLayers;

#[derive(SystemParam)]
pub(super) struct PrefabRenderLayers<'w, 's> {
    hierarchy: Query<
        'w,
        's,
        (
            Option<&'static ChildOf>,
            Option<&'static RenderLayers>,
            Option<&'static SuppressedModelRenderLayers>,
        ),
    >,
    cache: ResMut<'w, InheritedLayerCache>,
    path: Local<'s, Vec<Entity>>,
}

#[derive(Resource, Default)]
struct InheritedLayerCache {
    resolved: EntityHashMap<RenderLayers>,
    revision: u64,
}

impl InheritedLayerCache {
    fn invalidate(&mut self, entity: Entity, children: &Query<&Children>) {
        if self.resolved.remove(&entity).is_none() {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        if let Ok(descendants) = children.get(entity) {
            for child in descendants.iter() {
                self.invalidate(child, children);
            }
        }
    }
}

pub(super) fn register_inherited_render_layers(app: &mut App) {
    app.init_resource::<InheritedLayerCache>()
        .add_observer(|event: On<Insert<(ChildOf, RenderLayers, SuppressedModelRenderLayers)>>, mut cache: ResMut<InheritedLayerCache>, children: Query<&Children>| {
            cache.invalidate(event.entity, &children);
        })
        .add_observer(|event: On<Remove<(ChildOf, RenderLayers, SuppressedModelRenderLayers, GlobalTransform)>>, mut cache: ResMut<InheritedLayerCache>, children: Query<&Children>| {
            cache.invalidate(event.entity, &children);
        })
        .add_systems(PostUpdate, invalidate_edited_layers.before(super::orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers));
}

// Inserts, removals and reparenting are visible immediately, including between
// presentation systems. Also catch edits made in place by simulation systems.
fn invalidate_edited_layers(
    changed: Query<Entity, Or<(Changed<RenderLayers>, Changed<SuppressedModelRenderLayers>)>>,
    mut cache: ResMut<InheritedLayerCache>,
    children: Query<&Children>,
) {
    for entity in &changed {
        cache.invalidate(entity, &children);
    }
}

impl PrefabRenderLayers<'_, '_> {
    pub(super) fn revision(&self) -> u64 {
        self.cache.revision
    }

    pub(super) fn get(&mut self, mut entity: Entity) -> RenderLayers {
        let result = loop {
            if let Some(layers) = self.cache.resolved.get(&entity) {
                break layers.clone();
            }
            let Ok((parent, layers, saved)) = self.hierarchy.get(entity) else {
                break RenderLayers::default();
            };
            self.path.push(entity);
            // Suppressed draws still use the original camera policy for LOD and lighting.
            if let Some(layers) = saved.map_or(layers, |saved| saved.0.as_ref()) {
                break layers.clone();
            }
            let Some(parent) = parent else {
                break RenderLayers::default();
            };
            entity = parent.parent();
        };
        for visited in self.path.drain(..) {
            self.cache.resolved.insert(visited, result.clone());
        }
        result
    }
}
