use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::entity::EntityHashMap;
#[cfg(test)]
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::render::extract_instances::ExtractedInstances;
use bevy::render::render_phase::PhaseItem;
use bevy::render::render_phase::ViewSortedRenderPhases;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTransparentRenderStage;

pub(super) fn project_authored_effect_opaque_base_pass_order(
    materials: Res<Assets<EffectPassMaterial>>,
    mut passes: Query<
        (
            &MeshMaterial3d<EffectPassMaterial>,
            &mut AuthoredEffectTechniquePassSubmissionOrder,
        ),
        Or<(
            Changed<MeshMaterial3d<EffectPassMaterial>>,
            Added<AuthoredEffectTechniquePassSubmissionOrder>,
        )>,
    >,
) {
    for (handle, mut order) in &mut passes {
        if order.is_first_authored_pass() {
            if let Some(material) = materials.get(&handle.0) {
                order.update_opaque_base_pass(material.has_opaque_blend_state());
            }
        }
    }
}

pub(super) fn sort_authored_effect_invocations_before_bevy_mesh_batch_preparation(
    orders: Res<ExtractedInstances<AuthoredEffectTechniquePassSubmissionOrder>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut invocation_distances: Local<EntityHashMap<(bool, f32)>>,
) {
    for phase in phases.values_mut() {
        invocation_distances.clear();
        for item in phase.items.values() {
            if let Some(order) = orders.get(&item.main_entity()) {
                order.record_invocation_distance(&mut invocation_distances, item.distance);
            }
        }
        phase.items.sort_by(|_, left, _, right| {
            // Bevy PBR queues PLACEHOLDER as the render entity. Its retained
            // mesh instances and these ordering records use MainEntity keys.
            let left_order = orders.get(&left.main_entity());
            let right_order = orders.get(&right.main_entity());
            let left_distance = left_order
                .and_then(|order| order.invocation_distance(&invocation_distances))
                .unwrap_or((false, left.distance));
            let right_distance = right_order
                .and_then(|order| order.invocation_distance(&invocation_distances))
                .unwrap_or((false, right.distance));
            // Bevy's signed distances increase toward the camera, so ascending
            // order is back to front. Group every technique before
            // Bevy assigns its instance ranges and indirect draw parameters.
            // Opaque bases must precede depth-writing transparent shells.
            // Keep every pass of an invocation in the base pass's group.
            let stage = |order: Option<&AuthoredEffectTechniquePassSubmissionOrder>| {
                order.map_or(AuthoredEffectTransparentRenderStage::Scene, |order| {
                    order.render_stage()
                })
            };
            stage(left_order)
                .cmp(&stage(right_order))
                .then_with(|| right_distance.0.cmp(&left_distance.0))
                .then_with(|| left_distance.1.total_cmp(&right_distance.1))
                .then_with(|| match (left_order, right_order) {
                    (Some(left), Some(right)) => left.compare_hierarchy_order(right),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
        });
    }
}

#[cfg(test)]
mod authored_effect_ordering_tests {
    use super::*;
    use bevy::core_pipeline::core_3d::TransparentSortingInfo3d;
    use bevy::material::labels::DrawFunctionId;
    use bevy::render::render_phase::PhaseItemExtraIndex;
    use bevy::render::render_phase::SortedRenderPhase;
    use bevy::render::render_resource::CachedRenderPipelineId;
    use bevy::render::view::RetainedViewEntity;

    #[test]
    fn retained_mesh_placeholders_preserve_effect_stages_and_authored_pass_order() {
        let mut world = World::new();
        let near_base = world.spawn_empty().id();
        let near_overlay = world.spawn_empty().id();
        let far_base = world.spawn_empty().id();
        let far_overlay = world.spawn_empty().id();
        let caustic = world.spawn_empty().id();
        let terrain = world.spawn_empty().id();
        let water_base = world.spawn_empty().id();
        let water_overlay = world.spawn_empty().id();
        let view = RetainedViewEntity::new(world.spawn_empty().id().into(), None, 0);
        let mut orders =
            ExtractedInstances::<AuthoredEffectTechniquePassSubmissionOrder>::default();
        for (entity, invocation, index) in [
            (near_base, near_base, 0),
            (near_overlay, near_base, 1),
            (far_base, far_base, 0),
            (far_overlay, far_base, 1),
        ] {
            orders.insert(entity.into(), {
                let mut order =
                    AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index).unwrap();
                order.update_opaque_base_pass(index == 0);
                order
            });
        }
        for (entity, invocation, index, stage) in [
            (
                terrain,
                terrain,
                0,
                AuthoredEffectTransparentRenderStage::TerrainEffects,
            ),
            (
                caustic,
                caustic,
                0,
                AuthoredEffectTransparentRenderStage::TerrainCaustics,
            ),
            (
                water_base,
                water_base,
                0,
                AuthoredEffectTransparentRenderStage::WaterSurface,
            ),
            (
                water_overlay,
                water_base,
                1,
                AuthoredEffectTransparentRenderStage::WaterSurface,
            ),
        ] {
            orders.insert(entity.into(), {
                let mut order = AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                    .unwrap()
                    .with_render_stage(stage);
                order.update_opaque_base_pass(entity == water_base);
                order
            });
        }
        let mut phase = SortedRenderPhase::default();
        for (entity, distance) in [
            (near_overlay, -1.0),
            (far_overlay, -20.0),
            (near_base, -1.0),
            (far_base, -20.0),
            (water_overlay, -1000.0),
            (water_base, -1000.0),
            (caustic, -0.1),
            (terrain, 10.0),
        ] {
            phase.add_retained(Transparent3d {
                sorting_info: TransparentSortingInfo3d::AlwaysOnTop,
                distance,
                pipeline: CachedRenderPipelineId::INVALID,
                entity: (Entity::PLACEHOLDER, entity.into()),
                draw_function: DrawFunctionId(0),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: true,
            });
        }
        let mut phases = ViewSortedRenderPhases::<Transparent3d>::default();
        phases.insert(view, phase);
        world.insert_resource(phases);
        world.insert_resource(orders);
        assert!(world
            .run_system_once(sort_authored_effect_invocations_before_bevy_mesh_batch_preparation)
            .is_ok());
        let phases = world.resource::<ViewSortedRenderPhases<Transparent3d>>();
        assert_eq!(
            phases[&view]
                .items
                .values()
                .map(|item| item.main_entity().id())
                .collect::<Vec<_>>(),
            [
                terrain,
                caustic,
                water_base,
                water_overlay,
                far_base,
                far_overlay,
                near_base,
                near_overlay
            ]
        );
    }
}
