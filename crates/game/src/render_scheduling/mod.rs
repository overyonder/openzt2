use bevy::{
    asset::AssetEventSystems,
    camera::visibility::{calculate_bounds, VisibilitySystems},
    ecs::{
        query::AccessConflicts,
        schedule::{
            graph::{DiGraph, Direction},
            FlattenedDependencies, NodeId, ScheduleBuildError, ScheduleBuildPass, ScheduleGraph,
            SystemKey, SystemSetKey,
        },
        system::SystemAccess,
    },
    image::TextureAtlasLayout,
    light::{
        update_directional_light_frusta, update_point_light_frusta, update_spot_light_frusta,
        SimulationLightSystems,
    },
    mesh::{
        mark_2d_meshes_as_changed_if_their_assets_changed,
        mark_3d_meshes_as_changed_if_their_assets_changed, Mesh,
    },
    pbr::check_entities_needing_specialization,
    platform::hash::FixedHasher,
    prelude::*,
    sprite::update_text2d_layout,
    text::apply_text_edits,
    transform::TransformSystems,
    ui::widget::{text_system, update_editable_text_layout},
};
use indexmap::IndexSet;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world::model_render::ModelRenderCommandConsumers;

pub(super) fn configure(app: &mut App) {
    app.edit_schedule(PostUpdate, |schedule| {
        // Install before plugins supply the dependencies that Bevy uses to insert flushes.
        let settings = schedule.get_build_settings();
        let mut without_auto_flush = settings.clone();
        without_auto_flush.auto_insert_apply_deferred = false;
        schedule
            .set_build_settings(without_auto_flush)
            .add_build_pass(NarrowRenderDependencies)
            .set_build_settings(settings);
    });
    app.add_systems(
        PostUpdate,
        // Conditional barriers stay distinct from Bevy's automatic widget flushes.
        (
            ApplyDeferred
                .in_set(RenderInputFlush::MeshBounds)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::CameraCulling)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::VisibilityPropagation)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::LightClusters)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::DirectionalLightFrusta)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::ShadowVisibility)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::MarkHidden)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::MaterialBindings)
                .run_if(resource_exists::<Assets<EffectPassMaterial>>),
            ApplyDeferred
                .in_set(RenderInputFlush::GltfRelease)
                .run_if(resource_exists::<Assets<Mesh>>),
            ApplyDeferred
                .in_set(RenderInputFlush::MaterialSpecialization)
                .run_if(resource_exists::<Assets<Mesh>>),
        ),
    );
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RenderInputFlush {
    MeshBounds,
    CameraCulling,
    VisibilityPropagation,
    LightClusters,
    DirectionalLightFrusta,
    ShadowVisibility,
    MarkHidden,
    MaterialBindings,
    GltfRelease,
    MaterialSpecialization,
}

#[derive(Debug)]
struct NarrowRenderDependencies;

impl ScheduleBuildPass for NarrowRenderDependencies {
    fn add_dependency(&mut self, _: NodeId, _: NodeId, _: bool, _: bool) {}

    fn collapse_set(
        &mut self,
        _: SystemSetKey,
        _: &IndexSet<SystemKey, FixedHasher>,
        _: &DiGraph<NodeId>,
    ) -> impl Iterator<Item = (NodeId, NodeId)> {
        std::iter::empty()
    }

    fn build(
        &mut self,
        world: &mut World,
        graph: &mut ScheduleGraph,
        mut dependencies: FlattenedDependencies<'_>,
    ) -> Result<(), ScheduleBuildError> {
        let find_members = |set| {
            let mut pending = graph
                .system_sets
                .get_key(set)
                .map(NodeId::Set)
                .into_iter()
                .collect::<Vec<_>>();
            let mut members = Vec::new();
            while let Some(node) = pending.pop() {
                match node {
                    NodeId::System(key) => members.push(key),
                    NodeId::Set(_) => pending.extend(graph.hierarchy().graph().neighbors(node)),
                }
            }
            members
        };
        let publishers = find_members(AssetEventSystems.intern());
        let text2d_layout = find_members(update_text2d_layout.into_system_set().intern());
        let text_edits = find_members(apply_text_edits.into_system_set().intern());
        // Editable widgets maintain their own text buffers. World-space text layout
        // reads Text2d and TextSpan, with ordinary ECS access to the shared font context.
        for &layout in &text2d_layout {
            for &edits in &text_edits {
                dependencies.remove_edge(edits, layout);
            }
        }
        let text_layouts = text2d_layout
            .into_iter()
            .chain(find_members(text_system.into_system_set().intern()))
            .chain(find_members(
                update_editable_text_layout.into_system_set().intern(),
            ));
        for layout in text_layouts {
            let Some(system) = graph.systems.get(layout) else {
                continue;
            };
            for &publisher in &publishers {
                if graph
                    .systems
                    .get(publisher)
                    .is_some_and(|publisher| system.access().is_compatible(publisher.access()))
                {
                    dependencies.remove_edge(layout, publisher);
                }
            }
        }
        let bounds_systems = find_members(VisibilitySystems::CalculateBounds.intern());
        let sprite_asset_storage = [
            world.component_id::<Assets<Image>>(),
            world.component_id::<Assets<TextureAtlasLayout>>(),
        ];
        let mesh_event_consumers = bounds_systems
            .iter()
            .copied()
            .chain(find_members(
                mark_2d_meshes_as_changed_if_their_assets_changed
                    .into_system_set()
                    .intern(),
            ))
            .chain(find_members(
                mark_3d_meshes_as_changed_if_their_assets_changed
                    .into_system_set()
                    .intern(),
            ));
        let material_specialization = find_members(
            check_entities_needing_specialization::<StandardMaterial>
                .into_system_set()
                .intern(),
        )
        .into_iter()
        .chain(find_members(
            check_entities_needing_specialization::<EffectPassMaterial>
                .into_system_set()
                .intern(),
        ))
        .collect::<Vec<_>>();
        for consumer in mesh_event_consumers.chain(material_specialization.iter().copied()) {
            let Some(system) = graph.systems.get(consumer) else {
                continue;
            };
            for &publisher in &publishers {
                let Some(events) = graph.systems.get(publisher) else {
                    continue;
                };
                // Publishing image and atlas events drains their queues without
                // changing the asset values read by sprite bounds. Keep ordering
                // for consumers of the events or the published change ticks.
                let reads_only_sprite_asset_values = match (events.access(), system.access()) {
                    (SystemAccess::Shared(events), SystemAccess::Shared(consumer)) => {
                        match events
                            .combined_access()
                            .get_conflicts(consumer.combined_access())
                        {
                            AccessConflicts::Individual(ids) => ids
                                .iter()
                                .all(|id| sprite_asset_storage.contains(&Some(id))),
                            AccessConflicts::All => false,
                        }
                    }
                    _ => false,
                };
                if events.access().is_compatible(system.access()) || reads_only_sprite_asset_values
                {
                    dependencies.remove_edge(publisher, consumer);
                }
            }
        }
        let mesh_bounds = find_members(calculate_bounds.into_system_set().intern());
        if let [mesh_bounds] = mesh_bounds.as_slice() {
            let skinned_bounds = bounds_systems
                .into_iter()
                .filter(|&bounds| dependencies.graph().contains_edge(*mesh_bounds, bounds))
                .collect::<Vec<_>>();
            // Skinned bounds need the Aabb insertions from calculate_bounds.
            isolate_input_flush(
                &mut dependencies,
                &skinned_bounds,
                &find_members(RenderInputFlush::MeshBounds.intern()),
            );
        }
        // Keep the culling inputs together, including 2D bounds and frusta.
        isolate_input_flush(
            &mut dependencies,
            &find_members(VisibilitySystems::CheckVisibility.intern()),
            &find_members(RenderInputFlush::CameraCulling.intern()),
        );
        isolate_input_flush(
            &mut dependencies,
            &find_members(VisibilitySystems::VisibilityPropagate.intern()),
            &find_members(RenderInputFlush::VisibilityPropagation.intern()),
        );
        // Point and spot frusta already wait for transforms through clustering.
        // The direct edges let Bevy join them to the widget flush a second time.
        let clustering = find_members(SimulationLightSystems::AssignLightsToClusters.intern());
        let transforms = find_members(TransformSystems::Propagate.intern());
        for frustum in find_members(update_point_light_frusta.into_system_set().intern())
            .into_iter()
            .chain(find_members(
                update_spot_light_frusta.into_system_set().intern(),
            ))
        {
            for &transform in &transforms {
                if clustering.iter().any(|&assign| {
                    dependencies.graph().contains_edge(transform, assign)
                        && dependencies.graph().contains_edge(assign, frustum)
                }) {
                    dependencies.remove_edge(transform, frustum);
                }
            }
        }
        // Light preparation depends on the camera and shadow inputs. Keep its
        // command flushes separate from widget layout and clipping.
        for (consumers, flush) in [
            (
                SimulationLightSystems::AssignLightsToClusters.intern(),
                RenderInputFlush::LightClusters,
            ),
            (
                update_directional_light_frusta.into_system_set().intern(),
                RenderInputFlush::DirectionalLightFrusta,
            ),
            (
                SimulationLightSystems::CheckLightVisibility.intern(),
                RenderInputFlush::ShadowVisibility,
            ),
            (
                VisibilitySystems::MarkNewlyHiddenEntitiesInvisible.intern(),
                RenderInputFlush::MarkHidden,
            ),
        ] {
            isolate_input_flush(
                &mut dependencies,
                &find_members(consumers),
                &find_members(flush.intern()),
            );
        }
        // These readers need model commands, independently of widget commands.
        for (consumers, flush) in [
            (
                ModelRenderCommandConsumers::ProjectMaterialBindings,
                RenderInputFlush::MaterialBindings,
            ),
            (
                ModelRenderCommandConsumers::CompleteGltfRelease,
                RenderInputFlush::GltfRelease,
            ),
        ] {
            isolate_input_flush(
                &mut dependencies,
                &find_members(consumers.intern()),
                &find_members(flush.intern()),
            );
        }
        isolate_input_flush(
            &mut dependencies,
            &material_specialization,
            &find_members(RenderInputFlush::MaterialSpecialization.intern()),
        );
        Ok(())
    }
}

fn isolate_input_flush(
    dependencies: &mut FlattenedDependencies<'_>,
    consumers: &[SystemKey],
    flushes: &[SystemKey],
) {
    let [flush] = flushes else {
        return;
    };
    for &consumer in consumers {
        let inputs = dependencies
            .graph()
            .neighbors_directed(consumer, Direction::Incoming)
            .filter(|input| !consumers.contains(input))
            .collect::<Vec<_>>();
        for input in inputs {
            dependencies.remove_edge(input, consumer);
            dependencies.add_edge(input, *flush);
        }
        dependencies.add_edge(*flush, consumer);
    }
}
