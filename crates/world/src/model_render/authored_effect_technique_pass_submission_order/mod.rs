use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::entity::EntityHashMap;
use bevy::extract::extract_instances::ExtractedInstances;
use bevy::prelude::*;
use bevy::render::render_phase::ViewSortedRenderPhases;
use bevy::render::render_phase::{PhaseItem, SortedRenderPhase};
use bevy::render::view::ExtractedView;
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

pub(super) fn sort_transparent_effect_phase(
    orders: Res<
        ExtractedInstances<AuthoredEffectTechniquePassSubmissionOrder, bevy::render::RenderApp>,
    >,
    layers: Res<
        ExtractedInstances<
            super::environment_layer_rendering::EnvironmentPrimitiveLayer,
            bevy::render::RenderApp,
        >,
    >,
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut invocation_distances: Local<EntityHashMap<(bool, f32)>>,
) {
    for view in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        // The environment queue uses its own phase and prepares its own draws.
        // Exclude the duplicate while the stock material queue still targets Transparent3d.
        phase
            .items
            .retain(|_, item| !layers.contains_key(&item.main_entity()));
        phase.recalculate_sort_keys(view);
        sort_draws(phase, &orders, &mut invocation_distances);
    }
}

fn sort_draws(
    phase: &mut SortedRenderPhase<Transparent3d>,
    orders: &ExtractedInstances<
        AuthoredEffectTechniquePassSubmissionOrder,
        bevy::render::RenderApp,
    >,
    invocation_distances: &mut EntityHashMap<(bool, f32)>,
) {
    invocation_distances.clear();
    for item in phase.items.values() {
        if let Some(order) = orders.get(&item.main_entity()) {
            order.record_invocation_distance(invocation_distances, item.distance);
        }
    }
    phase.items.sort_by_cached_key(|_, item| {
        transparent_draw_sort_key(
            orders.get(&item.main_entity()),
            item.distance,
            invocation_distances,
        )
    });
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TransparentDrawSortKey {
    stage: AuthoredEffectTransparentRenderStage,
    transparent_base: bool,
    distance: u32,
    unauthored: bool,
    hierarchy_order: Option<(u64, usize, u64, u16)>,
}

pub(super) fn transparent_draw_sort_key(
    order: Option<&AuthoredEffectTechniquePassSubmissionOrder>,
    distance: f32,
    invocation_distances: &EntityHashMap<(bool, f32)>,
) -> TransparentDrawSortKey {
    let (opaque_base, distance) = order
        .and_then(|order| order.invocation_distance(invocation_distances))
        .unwrap_or((false, distance));
    // Map IEEE-754 total order onto an unsigned sort key.
    let bits = distance.to_bits();
    let distance = if distance.is_sign_negative() {
        !bits
    } else {
        bits ^ (1 << 31)
    };
    TransparentDrawSortKey {
        stage: order.map_or(AuthoredEffectTransparentRenderStage::Scene, |order| {
            order.render_stage()
        }),
        transparent_base: !opaque_base,
        distance,
        unauthored: order.is_none(),
        hierarchy_order: order.map(AuthoredEffectTechniquePassSubmissionOrder::hierarchy_order),
    }
}

#[cfg(test)]
mod authored_effect_ordering_tests {
    use super::*;
    use bevy::core_pipeline::core_3d::TransparentSortingInfo3d;
    use bevy::material::labels::DrawFunctionId;
    use bevy::render::render_phase::PhaseItemExtraIndex;
    use bevy::render::render_resource::CachedRenderPipelineId;

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
        let mut orders = ExtractedInstances::<
            AuthoredEffectTechniquePassSubmissionOrder,
            bevy::render::RenderApp,
        >::default();
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
        sort_draws(&mut phase, &orders, &mut EntityHashMap::default());
        assert_eq!(
            phase
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
