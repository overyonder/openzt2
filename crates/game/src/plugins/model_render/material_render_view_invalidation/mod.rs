//! Dependency changes which require material/view instance reconciliation.

use super::authored_model_material_pass_projection::{
    EffectPassMaterialRenderView, EffectPassMaterialRenderViewProjection, PerDrawEffectPassMaterial,
};
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use bevy::{
    camera::visibility::RenderLayers,
    ecs::system::SystemParam,
    mesh::{morph::MeshMorphWeights, skinning::SkinnedMesh},
    platform::collections::HashSet,
    prelude::*,
};

#[derive(SystemParam)]
pub(super) struct MaterialRenderViewInvalidation<'w, 's> {
    changed_sources: Query<
        'w,
        's,
        Entity,
        (
            With<PerDrawEffectPassMaterial>,
            Without<EffectPassMaterialRenderViewProjection>,
            Or<(
                Added<PerDrawEffectPassMaterial>,
                Changed<Mesh3d>,
                Changed<MeshMaterial3d<EffectPassMaterial>>,
                Changed<RenderLayers>,
                Changed<SkinnedMesh>,
                Changed<MeshMorphWeights>,
                Changed<AuthoredEffectTechniquePassSubmissionOrder>,
                Changed<ChildOf>,
            )>,
        ),
    >,
    source_materials: Query<
        'w,
        's,
        (Entity, &'static MeshMaterial3d<EffectPassMaterial>),
        (
            With<PerDrawEffectPassMaterial>,
            Without<EffectPassMaterialRenderViewProjection>,
        ),
    >,
    changed_views: Query<
        'w,
        's,
        (),
        (
            With<EffectPassMaterialRenderView>,
            Or<(
                Changed<EffectPassMaterialRenderView>,
                Added<Projection>,
                Added<GlobalTransform>,
            )>,
        ),
    >,
    removed_views: RemovedComponents<'w, 's, EffectPassMaterialRenderView>,
    removed_sources: RemovedComponents<'w, 's, PerDrawEffectPassMaterial>,
    removed_meshes: RemovedComponents<'w, 's, Mesh3d>,
    removed_materials: RemovedComponents<'w, 's, MeshMaterial3d<EffectPassMaterial>>,
    material_events: MessageReader<'w, 's, AssetEvent<EffectPassMaterial>>,
}

#[cfg(test)]
mod render_update_tests {
    use super::*;

    #[derive(Resource, Default)]
    struct ReconciliationSample {
        candidates: Vec<Entity>,
        sweep: bool,
        views_changed: bool,
    }

    fn sample(
        mut invalidation: MaterialRenderViewInvalidation,
        mut result: ResMut<ReconciliationSample>,
        mut changed: Local<HashSet<AssetId<EffectPassMaterial>>>,
    ) {
        let (sweep, views_changed) =
            invalidation.collect_affected_sources(&mut result.candidates, &mut changed);
        result.sweep = sweep;
        result.views_changed = views_changed;
    }

    #[test]
    fn render_update_reconciliation_is_idle_until_a_source_asset_or_view_changes() {
        let mut app = App::new();
        app.add_message::<AssetEvent<EffectPassMaterial>>()
            .init_resource::<ReconciliationSample>()
            .add_systems(Update, sample);
        let source = app
            .world_mut()
            .spawn((
                PerDrawEffectPassMaterial,
                Mesh3d::default(),
                MeshMaterial3d::<EffectPassMaterial>::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<ReconciliationSample>().candidates,
            vec![source]
        );
        app.update();
        assert!(app
            .world()
            .resource::<ReconciliationSample>()
            .candidates
            .is_empty());
        let view = app
            .world_mut()
            .spawn((
                EffectPassMaterialRenderView { render_layer: 6 },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.update();
        assert!(app.world().resource::<ReconciliationSample>().views_changed);
        assert_eq!(
            app.world().resource::<ReconciliationSample>().candidates,
            vec![source]
        );
        app.world_mut()
            .entity_mut(view)
            .insert(GlobalTransform::from_translation(Vec3::X));
        app.update();
        assert!(app
            .world()
            .resource::<ReconciliationSample>()
            .candidates
            .is_empty());
        app.world_mut()
            .write_message(AssetEvent::<EffectPassMaterial>::Modified {
                id: Handle::<EffectPassMaterial>::default().id(),
            });
        app.update();
        assert_eq!(
            app.world().resource::<ReconciliationSample>().candidates,
            vec![source]
        );
        app.world_mut().entity_mut(view).despawn();
        app.update();
        assert!(app.world().resource::<ReconciliationSample>().sweep);
        app.world_mut().entity_mut(source).remove::<Mesh3d>();
        app.update();
        assert!(app.world().resource::<ReconciliationSample>().sweep);
    }
}

impl MaterialRenderViewInvalidation<'_, '_> {
    /// Returns whether orphaned projections may exist and whether views changed.
    pub(super) fn collect_affected_sources(
        &mut self,
        candidates: &mut Vec<Entity>,
        changed_assets: &mut HashSet<AssetId<EffectPassMaterial>>,
    ) -> (bool, bool) {
        candidates.clear();
        changed_assets.clear();
        let removed_view = self.removed_views.read().count() != 0;
        let removed_source = (self.removed_sources.read().count() != 0)
            | (self.removed_meshes.read().count() != 0)
            | (self.removed_materials.read().count() != 0);
        let views_changed = removed_view || !self.changed_views.is_empty();
        for event in self.material_events.read() {
            match event {
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::Removed { id }
                | AssetEvent::LoadedWithDependencies { id } => {
                    changed_assets.insert(*id);
                }
                AssetEvent::Unused { .. } => {}
            }
        }
        if views_changed {
            candidates.extend(self.source_materials.iter().map(|(entity, _)| entity));
        } else {
            candidates.extend(self.changed_sources.iter());
            if !changed_assets.is_empty() {
                candidates.extend(
                    self.source_materials
                        .iter()
                        .filter_map(|(entity, material)| {
                            changed_assets.contains(&material.0.id()).then_some(entity)
                        }),
                );
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        (removed_view || removed_source, views_changed)
    }
}
