use openzt2_world_state::world_spawn::world_load_completion_marker::{
    WorldLoadCompletionSet, WorldLoadPreparationSet,
};
pub mod prefab_renderable_components;
pub mod prefab_world_instance_spawning;
// One-time translation from authored scenario and prefab rows into the live ECS.

pub mod challenge_economy_initialization;
pub mod expanding_column_presentation;
pub mod generated_entity_name_application;
pub(crate) mod named_physical_presentation_projection;
mod persistent_id_assignment;
mod physical_presentation_state;
mod prefab_attachment_independent_rotation;
mod prefab_authored_transform_animation_advancement;
mod prefab_object_presentation_attachment_projection;
pub mod prefab_presentation_render_tree;
pub mod prefab_socket_index;
mod prefab_terrain_decal_projection;

#[cfg(test)]
mod persistent_id_tests;
#[cfg(test)]
mod world_unload_tests;

mod starting_path_hydration;
mod starting_topology_hydration;
pub mod world_hydration_completion;
pub mod world_load_request_acceptance;
pub mod world_persistent_id_reservation;
mod world_prefab_record_hydration;
mod world_spawn_value_application;
mod world_terrain_hydration;
pub mod world_unloading;
mod zoo_entrance_anchor_synchronization;

use crate::world_spawn::named_physical_presentation_projection::{
    attach_named_presentations_to_model_nodes, hydrate_required_named_physical_presentations,
    project_physical_presentation_caustic_policies, select_named_presentation_fade_states,
};
use crate::world_spawn::persistent_id_assignment::assign_requested_persistent_ids;
use crate::world_spawn::prefab_authored_transform_animation_advancement::advance_authored_prefab_transform_animations_from_elapsed_time;
use crate::world_spawn::prefab_object_presentation_attachment_projection::{
    initialize_authored_physical_presentation_controllers,
    project_pending_physical_presentation_children,
};
use crate::world_spawn::prefab_presentation_render_tree::hydrate_prefab_presentations;
use crate::world_spawn::prefab_world_instance_spawning::{
    begin_rebuilding_world_instance_renderables_after_prefab_reload,
    finish_rebuilding_world_instance_renderables,
};
use crate::world_spawn::starting_path_hydration::hydrate_all_starting_paths_into_topology_once;
use crate::world_spawn::starting_topology_hydration::hydrate_all_starting_fence_nodes_and_edges_into_topology_once;
use crate::world_spawn::world_hydration_completion::complete_or_fail_finished_world_hydration;
use crate::world_spawn::world_load_request_acceptance::{
    accept_queued_world_load_after_required_assets_resolve, admit_first_world_load_request,
    PendingWorldLoad,
};
use crate::world_spawn::world_persistent_id_reservation::reserve_all_imported_world_persistent_ids;
use crate::world_spawn::world_prefab_record_hydration::{
    admit_starting_prefab_rows, commit_ready_starting_prefab_rows_in_source_order,
    demand_starting_prefab_dependencies, mark_ready_starting_prefab_dependencies,
    StartingPrefabDependencies, StartingPrefabDependenciesReady, StartingPrefabRow,
};
use crate::world_spawn::world_spawn_value_application::apply_all_authored_world_spawn_values_once;
use crate::world_spawn::world_terrain_hydration::hydrate_selected_terrain_chunks_world_bounds_and_topology_grid;
use crate::world_spawn::world_unloading::despawn_all_world_roots_and_members_when_leaving_ingame;
use crate::world_spawn::world_unloading::unload_requested_worlds_and_their_members;
use crate::world_spawn::zoo_entrance_anchor_synchronization::synchronize_zoo_entrance_anchor_after_transform_changes;

pub mod prefab_effect_trigger_hydration;
mod world_entity_crating;
mod world_object_selection;

use bevy::asset::AssetLoadFailedEvent;
use bevy::ecs::schedule::SystemCondition;
use bevy::gltf::Gltf;
use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::world_load_completion_marker::ImportedPersistentIdsReserved;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use challenge_economy_initialization::{ChallengeEconomyInitializer, ChallengeEconomyInitializerLoader};

pub struct WorldSpawnPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LoadingWorldDocumentReady;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct IndependentAttachmentOrientation;

impl Plugin for WorldSpawnPlugin {
    fn build(&self, app: &mut App) {
        prefab_terrain_decal_projection::register_prefab_terrain_decals(app);
        app.add_plugins(openzt2_world_state::world_spawn::world_membership_types::target_type_index::WorldTargetTypeIndexPlugin);
        app.add_observer(generated_entity_name_application::name_requested_world_entity)
            .add_observer(generated_entity_name_application::assign_pending_world_entity_name);
        app.init_asset::<ChallengeEconomyInitializer>()
            .init_asset_loader::<ChallengeEconomyInitializerLoader>();
        use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndexPlugin;
        if !app.is_plugin_added::<PersistentIdIndexPlugin>() {
            app.add_plugins(PersistentIdIndexPlugin);
        }
        app.add_observer(world_hydration_completion::conclude_starting_animal_projection)
            .add_observer(world_hydration_completion::conclude_terrain_mesh_projection)
            .add_observer(world_hydration_completion::conclude_rejected_terrain_mesh_projection);
        app.init_resource::<openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream>();
        app.add_systems(
            Update,
            (
                generated_entity_name_application::initialize_world_entity_name_counters,
                generated_entity_name_application::register_and_name_spawned_world_entities,
            )
                .chain()
                .in_set(GameSet::Intent)
                .run_if(in_state(GamePhase::InGame)),
        );
        app.configure_sets(
            Update,
            (
                WorldLoadPreparationSet::Accept,
                WorldLoadPreparationSet::InitializeClock.in_set(LoadingWorldDocumentReady),
                WorldLoadPreparationSet::HydrateTerrain,
            )
                .chain(),
        );
        app.configure_sets(
            Update,
            LoadingWorldDocumentReady.run_if(world_hydration_completion::loading_screen_is_ready),
        );
        app.add_message::<openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest>()
            .add_message::<openzt2_world_state::world_spawn::prefab_model_replacement::PrefabModelReplaced>()
            .add_message::<prefab_presentation_render_tree::ReplacedPrefabReloaded>();
        app.add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::AssignPersistentId>()
            .add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssigned>()
            .add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssignmentFailed>()
            .add_message::<openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad>()
            .add_message::<openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished>()
            .add_message::<openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed>()
            .add_message::<openzt2_world_state::world_spawn::world_unloading::UnloadWorld>()
            .add_message::<openzt2_world_state::world_spawn::world_entity_crating::RemoveWorldEntityFromCrate>()
            .add_systems(
                Update,
                (
                    (
                        admit_first_world_load_request.run_if(on_message::<openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad>),
                        accept_queued_world_load_after_required_assets_resolve.run_if(
                            any_match_filter::<Added<PendingWorldLoad>>
                                .or_eager(resource_changed::<WorldDefinitions>)
                                .or_eager(resource_changed::<WorldScenarios>)
                                .or_eager(on_message::<AssetEvent<WorldScenarioDocumentAsset>>)
                                .or_eager(on_message::<AssetLoadFailedEvent<WorldScenarioDocumentAsset>>)
                                .or_eager(on_message::<AssetEvent<ChallengeEconomyInitializer>>)
                                .or_eager(on_message::<AssetLoadFailedEvent<ChallengeEconomyInitializer>>)
                                .or_eager(|mut removed: RemovedComponents<WorldRoot>| removed.read().count() != 0),
                        ),
                    ).chain().in_set(WorldLoadPreparationSet::Accept),
                    (
                        hydrate_selected_terrain_chunks_world_bounds_and_topology_grid.in_set(WorldLoadPreparationSet::HydrateTerrain),
                        reserve_all_imported_world_persistent_ids.after(WorldLoadPreparationSet::InitializeClock),
                        (
                            hydrate_all_starting_fence_nodes_and_edges_into_topology_once,
                            hydrate_all_starting_paths_into_topology_once,
                        )
                            .after(WorldLoadPreparationSet::HydrateTerrain)
                            .after(reserve_all_imported_world_persistent_ids)
                            .run_if(any_with_component::<openzt2_zoo_state::topology::topology_graph_types::TopologyGrid>),
                        (
                            admit_starting_prefab_rows.run_if(
                                any_match_filter::<Or<(Added<ImportedPersistentIdsReserved>, Changed<openzt2_world_state::world_spawn::world_hydration_types::WorldHydration>)>>,
                            ),
                            demand_starting_prefab_dependencies.run_if(
                                any_match_filter::<Added<StartingPrefabRow>>
                                    .or_eager(resource_changed::<WorldDefinitions>)
                                    .or_eager(resource_changed::<SpeciesAssets>),
                            ),
                            mark_ready_starting_prefab_dependencies.run_if(
                                any_match_filter::<Added<StartingPrefabDependencies>>
                                    .or_eager(on_message::<AssetEvent<ScenePrefabAsset>>)
                                    .or_eager(on_message::<AssetEvent<Gltf>>)
                                    .or_eager(on_message::<AssetLoadFailedEvent<ScenePrefabAsset>>)
                                    .or_eager(on_message::<AssetLoadFailedEvent<Gltf>>),
                            ),
                            commit_ready_starting_prefab_rows_in_source_order.run_if(
                                any_match_filter::<Added<ImportedPersistentIdsReserved>>
                                    .or_eager(any_match_filter::<(With<StartingPrefabRow>, Or<(Added<StartingPrefabDependenciesReady>, Added<WorldLoadFailure>)>)>)
                                    .or_eager(resource_changed::<WorldDefinitions>),
                            ),
                            apply_all_authored_world_spawn_values_once,
                        ).chain().after(reserve_all_imported_world_persistent_ids),
                    )
                        .before(WorldLoadCompletionSet)
                        .in_set(LoadingWorldDocumentReady),
                    complete_or_fail_finished_world_hydration.in_set(WorldLoadCompletionSet),
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                (
                    unload_requested_worlds_and_their_members
                        .before(WorldLoadPreparationSet::Accept),
                    crate::world_spawn::world_entity_crating::remove_requested_world_entities_from_crates,
                    world_object_selection::select_world_object_on_pointer_press,
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                PostUpdate,
                world_object_selection::mark_inspectable_meshes_for_picking
                    .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
            )
            .add_systems(
                Update,
                (
                    advance_authored_prefab_transform_animations_from_elapsed_time,
                    crate::world_spawn::world_entity_crating::synchronize_contained_animal_transforms,
                )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                crate::world_spawn::named_physical_presentation_projection::project_named_presentation_animation_owners
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    prefab_presentation_render_tree::publish_replaced_prefab_reloads,
                    prefab_presentation_render_tree::rebuild_prefab_presentations_after_prefab_reload
                        .after_ignore_deferred(prefab_presentation_render_tree::publish_replaced_prefab_reloads),
                    begin_rebuilding_world_instance_renderables_after_prefab_reload
                        .after_ignore_deferred(prefab_presentation_render_tree::publish_replaced_prefab_reloads),
                    finish_rebuilding_world_instance_renderables
                        .after(begin_rebuilding_world_instance_renderables_after_prefab_reload),
                    hydrate_prefab_presentations
                        .run_if(
                            any_match_filter::<Changed<openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation>>
                                .or_eager(on_message::<AssetEvent<ScenePrefabAsset>>)
                                .or_eager(on_message::<AssetLoadFailedEvent<ScenePrefabAsset>>)
                                .or_eager(any_component_removed::<openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentationHydrated>),
                        )
                        .after(prefab_presentation_render_tree::rebuild_prefab_presentations_after_prefab_reload),
                    crate::world_spawn::expanding_column_presentation::align_expanding_column_piece_attachments
                        .run_if(
                            any_match_filter::<Added<expanding_column_presentation::PendingExpandingColumnPieceAttachment>>
                                .or_eager(on_message::<AssetEvent<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>),
                        ),
                    initialize_authored_physical_presentation_controllers,
                    prefab_socket_index::project_prefab_socket_indexes
                        .after(initialize_authored_physical_presentation_controllers)
                        .after(hydrate_required_named_physical_presentations)
                        .after(finish_rebuilding_world_instance_renderables)
                        .after(hydrate_prefab_presentations),
                    crate::world_spawn::physical_presentation_state::apply_physical_presentation_requests
                        .after(initialize_authored_physical_presentation_controllers),
                    project_pending_physical_presentation_children
                        .after(prefab_socket_index::project_prefab_socket_indexes)
                        .after(crate::world_spawn::physical_presentation_state::apply_physical_presentation_requests)
                        .after(hydrate_prefab_presentations)
                        .after(finish_rebuilding_world_instance_renderables),
                    hydrate_required_named_physical_presentations
                        .after(commit_ready_starting_prefab_rows_in_source_order),
                    attach_named_presentations_to_model_nodes
                        .after(prefab_socket_index::project_prefab_socket_indexes)
                        .after(hydrate_required_named_physical_presentations)
                        .after(finish_rebuilding_world_instance_renderables)
                        .after(hydrate_prefab_presentations),
                    named_physical_presentation_projection::seed_named_presentation_fade_inputs
                        .after(hydrate_required_named_physical_presentations),
                    select_named_presentation_fade_states
                        .after(named_physical_presentation_projection::seed_named_presentation_fade_inputs),
                    project_physical_presentation_caustic_policies
                        .after(attach_named_presentations_to_model_nodes)
                        .after(project_pending_physical_presentation_children),
                    prefab_effect_trigger_hydration::hydrate_prefab_effect_triggers.run_if(
                        any_match_filter::<Changed<prefab_effect_trigger_hydration::PrefabEffectTrigger>>
                            .or_eager(on_message::<AssetEvent<openzt2_game_assets::effect::ParticleEffectDocumentAsset>>)
                            .or_eager(on_message::<AssetLoadFailedEvent<openzt2_game_assets::effect::ParticleEffectDocumentAsset>>),
                    ),
                    synchronize_zoo_entrance_anchor_after_transform_changes
                        .after(commit_ready_starting_prefab_rows_in_source_order),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                PostUpdate,
                named_physical_presentation_projection::advance_named_presentation_opacity
                    .before(bevy::app::PropagateSet::<prefab_world_instance_spawning::morph_fade_projection::NamedPrefabOpacity>::default())
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(
                PostUpdate,
                crate::world_spawn::prefab_attachment_independent_rotation::preserve_independent_attachment_orientation
                    .after(bevy::app::AnimationSystems)
                    .after(openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection)
                    .in_set(IndependentAttachmentOrientation)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                despawn_all_world_roots_and_members_when_leaving_ingame
                    .in_set(openzt2_world_state::world_spawn::world_unloading::WorldUnloadSet),
            )
            .add_systems(
                FixedUpdate,
                crate::world_spawn::physical_presentation_state::child_lifecycle::advance_authored_child_lifetimes
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                assign_requested_persistent_ids
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}
