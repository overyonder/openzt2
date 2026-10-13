use std::any::TypeId;

use bevy::{
    animation::{
        graph::{AnimationGraphHandle, AnimationNodeType},
        AnimatedBy, AnimationTargetId,
    },
    camera::{
        primitives::Aabb,
        visibility::{RenderLayers, VisibleEntities},
        RenderTarget,
    },
    ecs::{query::QueryData, system::SystemParam},
    gltf::GltfMaterialName,
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    prelude::*,
    window::PrimaryWindow,
};

use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationSetAsset,
    material::runtime::effect_pass_gpu_data::EffectPassMaterial,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets, SpeciesView},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_information::catalogue::catalogue_preview_scene_projection::{
    CataloguePreviewOwner, CataloguePreviewScene,
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_world::model_render::prefab_model_tint_projection::ProjectedPrefabTintMesh;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::AnimalPresentationOwner;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::animal_lifecycle::types::SelectedAnimalPresentationModel;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewOwner;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRenderable;

use super::{
    verification_journey_state_types::{VerificationCaptureProgress, VerificationJourneySession},
    verification_journey_types::VerificationJourneyAction,
};

mod material_observations;
mod skin_observations;

const MESH_SAMPLE_LIMIT: usize = 4;
const CAMERA_SAMPLE_LIMIT: usize = 4;
const HIERARCHY_SAMPLE_LIMIT: usize = 8;

#[derive(QueryData)]
struct PreviewNodeObservation {
    name: Option<&'static Name>,
    local: Option<&'static Transform>,
    global: Option<&'static GlobalTransform>,
    // Observe the existing component, not evaluated or deformed skin bounds.
    bounds: Option<&'static Aabb>,
    layers: Option<&'static RenderLayers>,
    visibility: Option<&'static Visibility>,
    inherited: Option<&'static InheritedVisibility>,
}

#[derive(QueryData)]
struct PreviewModelObservation {
    model: &'static PrefabModel,
    owner: Option<&'static ObjectPlacementPreviewOwner>,
    animal_owner: Option<&'static AnimalPresentationOwner>,
    renderable: Has<ObjectPlacementPreviewRenderable>,
    controller: Option<&'static AnimationPlaybackController>,
    player: Option<&'static AnimationPlayer>,
    graph: Option<&'static AnimationGraphHandle>,
    pending: Option<&'static PendingModelAnimationAssets>,
    resolved: Has<ModelAnimationAttachmentResolved>,
}

#[derive(QueryData)]
struct PreviewDrawObservation {
    mesh: &'static Mesh3d,
    skin: Option<&'static SkinnedMesh>,
    standard: Option<&'static MeshMaterial3d<StandardMaterial>>,
    effect: Option<&'static MeshMaterial3d<EffectPassMaterial>>,
    pass_order: Option<&'static AuthoredEffectTechniquePassSubmissionOrder>,
    authored_material_name: Option<&'static GltfMaterialName>,
    tint_projection: Option<&'static ProjectedPrefabTintMesh>,
}

#[derive(QueryData)]
struct PlacedAnimalObservation {
    entity: Entity,
    definition: &'static DefinitionId,
    species: &'static SpeciesHandle,
    variant: &'static AnimalVariant,
    model: &'static SelectedAnimalPresentationModel,
    persistent_id: &'static PersistentId,
    world: &'static WorldMember,
}

#[derive(QueryData)]
struct PreviewCameraObservation {
    entity: Entity,
    camera: &'static Camera,
    target: &'static RenderTarget,
    projection: Option<&'static Projection>,
    visible: Option<&'static VisibleEntities>,
    parent: Option<&'static ChildOf>,
    zoo: Has<ZooCamera>,
    portrait_owner: Has<CataloguePreviewOwner>,
}

#[derive(SystemParam)]
pub(super) struct AnimalPreviewCaptureQueries<'w, 's> {
    selected: Res<'w, SelectedCatalogueEntry>,
    species: Option<Res<'w, SpeciesAssets>>,
    species_assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    animals: Query<'w, 's, PlacedAnimalObservation, With<Animal>>,
    worlds: Query<'w, 's, Entity, (With<WorldRoot>, With<SelectedWorldIdentity>)>,
    tints: Query<'w, 's, &'static PrefabModelTint>,
    previews: Query<
        'w,
        's,
        (
            Entity,
            &'static ConstructionPreview,
            Option<&'static ObjectPlacementPrefabSource>,
        ),
    >,
    portraits: Query<
        'w,
        's,
        (
            Entity,
            &'static CataloguePreviewScene,
            Option<&'static ImageNode>,
            Option<&'static UiDocumentOwner>,
        ),
    >,
    nodes: Query<'w, 's, PreviewNodeObservation>,
    models: Query<'w, 's, PreviewModelObservation>,
    draws: Query<'w, 's, PreviewDrawObservation>,
    cameras:
        Query<'w, 's, PreviewCameraObservation, Or<(With<ZooCamera>, With<CataloguePreviewOwner>)>>,
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    joints: Query<
        'w,
        's,
        (
            Option<&'static AnimationJointTarget>,
            Option<&'static AnimatedBy>,
            Option<&'static AnimationTargetId>,
        ),
    >,
    meshes: Option<Res<'w, Assets<Mesh>>>,
    inverse_binds: Option<Res<'w, Assets<SkinnedMeshInverseBindposes>>>,
    animation_sets: Option<Res<'w, Assets<AnimationSetAsset>>>,
    animation_clips: Option<Res<'w, Assets<AnimationClip>>>,
    animation_graphs: Option<Res<'w, Assets<AnimationGraph>>>,
    standard_materials: Option<Res<'w, Assets<StandardMaterial>>>,
    effect_materials: Option<Res<'w, Assets<EffectPassMaterial>>>,
    images: Option<Res<'w, Assets<Image>>>,
    server: Res<'w, AssetServer>,
    windows: Query<'w, 's, Entity, With<PrimaryWindow>>,
    simulation: Option<Res<'w, SimulationControl>>,
    virtual_time: Res<'w, Time<Virtual>>,
    fixed_time: Res<'w, Time<Fixed>>,
    real_time: Res<'w, Time<Real>>,
}

pub(super) fn log_selected_animal_preview_at_capture_checkpoints(
    session: Res<VerificationJourneySession>,
    queries: AnimalPreviewCaptureQueries,
) {
    // Match the existing CPU presentation diagnostic checkpoints. In particular,
    // failure requests may have no preparation checkpoint. This is not readback.
    let stage = match session.step_capture {
        VerificationCaptureProgress::Preparing { .. } if session.step_frame == 1 => "preparing",
        VerificationCaptureProgress::Requested { frame, .. } if session.step_frame == frame + 1 => {
            "requested"
        }
        _ => return,
    };
    let capture = session
        .steps
        .get(session.step_index)
        .and_then(|step| match &step.action {
            VerificationJourneyAction::Capture(name) => Some(name.as_str()),
            _ => None,
        });
    // Completing adoption enters Inspect and clears catalogue selection. A sole
    // live animal can still be compared without retaining an adoption registry.
    let world = queries.worlds.single().ok();
    let definition = queries.selected.0.or_else(|| {
        let mut animals = queries.animals.iter().filter(|animal| Some(animal.world.root) == world);
        let first = animals.next();
        let count = usize::from(first.is_some()) + animals.count();
        info!(target: "openzt2_verification", stage, frame = session.frames_run, ?capture, ?world,
            animals = count, selection_rule = "catalogue absent: sole live animal in sole current world; ambiguity omitted",
            "animal preview placed comparison subject resolution");
        first.filter(|_| count == 1).map(|animal| animal.definition.0)
    });
    let Some(definition) = definition else {
        return;
    };
    let span = info_span!(target: "openzt2_verification", "animal preview CPU checkpoint",
        stage, frame = session.frames_run, step_frame = session.step_frame, step = session.step_index,
        journey = ?session.journeys.get(session.journey_index).map(|journey| journey.name.as_str()), ?capture, ?definition);
    let _checkpoint = span.enter();
    let species = queries
        .species
        .as_ref()
        .zip(queries.species_assets.as_ref())
        .and_then(|(index, assets)| index.get(assets));
    let Some(species) = species else {
        info!(target: "openzt2_verification", "animal preview selected subject resolution unavailable; no descendants observed");
        return;
    };
    let root_species = species.find(definition);
    let variant = species.find_variant_with_species(definition);
    if root_species.is_none() && variant.is_none() {
        return;
    }
    let canonical_definition = queries
        .active_definitions
        .get(&queries.definitions)
        .is_some_and(|catalogue| catalogue.find_object(definition).is_some());
    info!(target: "openzt2_verification", stage, frame = session.frames_run,
        step_frame = session.step_frame, step = session.step_index, ?capture, ?definition,
        capture_target = ?session.capture_target,
        catalogue_definition = ?queries.selected.0,
        "animal preview CPU checkpoint context; records inherit the checkpoint span");
    info!(target: "openzt2_verification", canonical_definition,
        species = ?root_species.map(|root| root.id).or_else(|| variant.map(|(root, _)| root.id)),
        variant = ?variant.map(|(_, variant)| variant.id),
        simulation_paused = ?queries.simulation.as_ref().map(|control| control.paused),
        virtual_paused = queries.virtual_time.is_paused(), virtual_speed = queries.virtual_time.relative_speed(),
        virtual_speed_finite = queries.virtual_time.relative_speed().is_finite(),
        virtual_seconds = queries.virtual_time.elapsed_secs_f64(), fixed_seconds = queries.fixed_time.elapsed_secs_f64(),
        real_seconds = queries.real_time.elapsed_secs_f64(), "animal preview selected subject and clocks");
    info!(target: "openzt2_verification", mesh_store = queries.meshes.is_some(), inverse_bind_store = queries.inverse_binds.is_some(),
        animation_set_store = queries.animation_sets.is_some(), animation_clip_store = queries.animation_clips.is_some(),
        animation_graph_store = queries.animation_graphs.is_some(),
        standard_material_store = queries.standard_materials.is_some(), effect_material_store = queries.effect_materials.is_some(),
        image_store = queries.images.is_some(), "animal preview CPU asset stores available");
    if !canonical_definition {
        return;
    }
    queries.log_selected_animal_subjects(world, &species, root_species, definition, &session);
}

fn transform_is_finite(transform: &Transform) -> bool {
    transform.translation.is_finite()
        && transform.rotation.is_finite()
        && transform.scale.is_finite()
}

impl AnimalPreviewCaptureQueries<'_, '_> {
    fn log_selected_animal_subjects(
        &self,
        world: Option<Entity>,
        species: &SpeciesView<'_>,
        root_species: Option<&openzt2_game_data::species::Species>,
        definition: openzt2_game_data::AssetId,
        session: &VerificationJourneySession,
    ) {
        let mut placements = 0_usize;
        for (root, preview, prefab) in &self.previews {
            if preview.definition != definition {
                continue;
            }
            placements += 1;
            info!(target: "openzt2_verification", ?root, authored_transform = ?preview.transform,
            authored_transform_finite = transform_is_finite(&preview.transform),
            prefab = ?prefab.map(|source| source.0.id()),
            prefab_path = ?prefab.and_then(|source| self.server.get_path(source.0.id())),
            prefab_load_state = ?prefab.and_then(|source| self.server.get_load_state(source.0.id())),
            "animal preview placement owner");
            self.log_subject(root, false, false, None, session);
        }
        let mut portraits = 0_usize;
        for (image_owner, portrait, image, document) in &self.portraits {
            if portrait.definition() != definition {
                continue;
            }
            portraits += 1;
            info!(target: "openzt2_verification", ?image_owner, document = ?document.map(|owner| owner.0),
            scene = ?portrait.scene(), image = ?image.map(|node| node.image.id()),
            image_loaded = ?image.map(|node| self.images.as_ref().is_some_and(|images| images.get(&node.image).is_some())),
            "animal preview portrait image owner");
            self.log_node(image_owner);
            self.log_subject(portrait.scene(), true, false, image, session);
        }
        // No adoption history is retained: observe only an unambiguous live match
        // in the current world. The ordinary single-animal journey supplies this case.
        let mut matching_animals = self.animals.iter().filter(|animal| {
            Some(animal.world.root) == world
                && species
                    .find_variant_with_species(animal.variant.0)
                    .is_some_and(|(root, variant)| {
                        root.id == animal.species.species
                            && root.world_definition == animal.definition.0
                            && (root_species.is_some_and(|selected| selected.id == root.id)
                                || definition == variant.id)
                    })
        });
        let first = matching_animals.next();
        let matching_animals_count = usize::from(first.is_some()) + matching_animals.count();
        info!(target: "openzt2_verification", ?world, matching_animals_count,
        selection_rule = "unique canonical selected species/variant in current world; ambiguous matches omitted; adoption origin not retained",
        "animal preview placed comparison selection");
        if matching_animals_count == 1 {
            if let Some(animal) = first {
                info!(target: "openzt2_verification", entity = ?animal.entity,
                persistent_id = animal.persistent_id.0, definition = ?animal.definition.0,
                species = ?animal.species.species, variant = ?animal.variant.0,
                selected_model = ?animal.model.model, world = ?animal.world.root,
                canonical_model_matches = ?species.find_variant(animal.variant.0)
                    .map(|variant| openzt2_game_data::AssetId::from_virtual_path(&variant.model) == animal.model.model),
                "animal preview placed comparison identity; scale is the live root transform");
                self.log_subject(animal.entity, false, true, None, session);
            }
        }
        info!(target: "openzt2_verification", placements, portraits, "animal preview observed owner totals");
    }

    fn log_node(&self, entity: Entity) {
        let node = self.nodes.get(entity).ok();
        info!(target: "openzt2_verification", ?entity, entity_exists = node.is_some(),
            name = ?node.as_ref().and_then(|node| node.name),
            local = ?node.as_ref().and_then(|node| node.local),
            local_finite = ?node.as_ref().and_then(|node| node.local).map(transform_is_finite),
            global = ?node.as_ref().and_then(|node| node.global).map(GlobalTransform::to_matrix),
            global_finite = ?node.as_ref().and_then(|node| node.global).map(|global| global.to_matrix().is_finite()),
            aabb = ?node.as_ref().and_then(|node| node.bounds),
            aabb_finite = ?node.as_ref().and_then(|node| node.bounds).map(|bounds| bounds.center.is_finite() && bounds.half_extents.is_finite()),
            layers = ?node.as_ref().and_then(|node| node.layers),
            visibility = ?node.as_ref().and_then(|node| node.visibility),
            inherited_visible = ?node.as_ref().and_then(|node| node.inherited).map(|visibility| visibility.get()),
            parent = ?self.parents.get(entity).ok().map(|parent| parent.parent()),
            "animal preview node transforms and existing bounds");
    }

    fn matching_camera(
        &self,
        camera: &PreviewCameraObservationItem<'_, '_>,
        root: Entity,
        portrait: bool,
        session: &VerificationJourneySession,
    ) -> bool {
        if portrait {
            return camera.portrait_owner
                && camera.parent.is_some_and(|parent| parent.parent() == root);
        }
        let window = self.windows.single().ok();
        let target = session
            .capture_target
            .as_ref()
            .and_then(|target| target.normalize(window));
        camera.zoo
            && camera.camera.is_active
            && target.is_some()
            && camera.target.normalize(window) == target
    }

    fn log_subject(
        &self,
        root: Entity,
        portrait: bool,
        placed: bool,
        image: Option<&ImageNode>,
        session: &VerificationJourneySession,
    ) {
        let span = info_span!(target: "openzt2_verification", "animal preview render subject", ?root, portrait, placed);
        let _subject = span.enter();
        self.log_node(root);
        let camera_count = self
            .cameras
            .iter()
            .filter(|camera| self.matching_camera(camera, root, portrait, session))
            .count();
        for camera in self
            .cameras
            .iter()
            .filter(|camera| self.matching_camera(camera, root, portrait, session))
            .take(CAMERA_SAMPLE_LIMIT)
        {
            info!(target: "openzt2_verification", camera = ?camera.entity, active = camera.camera.is_active,
                target = ?camera.target, projection = ?camera.projection,
                viewport = ?camera.camera.viewport, sub_camera_view = ?camera.camera.sub_camera_view,
                order = camera.camera.order, clip_from_view = ?camera.camera.clip_from_view(),
                projection_matrix_finite = camera.camera.clip_from_view().is_finite(),
                portrait_image_target_matches = ?image.map(|image| match camera.target {
                    RenderTarget::Image(target) => target.handle.id() == image.image.id(),
                    _ => false,
                }),
                visible_entities_present = camera.visible.is_some(), "animal preview corresponding camera");
            self.log_node(camera.entity);
        }
        let mut model_count = 0_usize;
        let mut draw_count = 0_usize;
        let mut excluded_draw_count = 0_usize;
        let mut skin_totals = skin_observations::JointObservationTotals::default();
        // Traverse only this live selected owner's hierarchy. Totals scale with
        // its descendants and skin joints; only detailed records are capped.
        for entity in self.children.iter_descendants_depth_first::<Children>(root) {
            if let Ok(model) = self.models.get(entity) {
                if portrait
                    || (placed && model.animal_owner.is_some_and(|owner| owner.0 == root))
                    || (model.renderable && model.owner.is_some_and(|owner| owner.0 == root))
                {
                    model_count += 1;
                    if model_count <= MESH_SAMPLE_LIMIT {
                        self.log_model(entity, &model);
                    }
                }
            }
            let Ok(draw) = self.draws.get(entity) else {
                continue;
            };
            let model_entity = std::iter::once(entity)
                .chain(self.parents.iter_ancestors(entity))
                .take_while(|ancestor| *ancestor != root)
                .find(|ancestor| {
                    self.models.get(*ancestor).is_ok_and(|model| {
                        portrait
                            || (placed && model.animal_owner.is_some_and(|owner| owner.0 == root))
                            || (model.renderable
                                && model.owner.is_some_and(|owner| owner.0 == root))
                    })
                });
            let Some(model_entity) = model_entity else {
                excluded_draw_count += 1;
                continue;
            };
            draw_count += 1;
            let detailed = draw_count <= MESH_SAMPLE_LIMIT;
            if let Some(skin) = draw.skin {
                skin_totals.add(self.observe_skin(entity, model_entity, skin, detailed));
            }
            if !detailed {
                continue;
            }
            let span = info_span!(target: "openzt2_verification", "animal preview draw", mesh_entity = ?entity, ?model_entity);
            let _draw = span.enter();
            let loaded = self
                .meshes
                .as_ref()
                .and_then(|meshes| meshes.get(&draw.mesh.0));
            info!(target: "openzt2_verification", mesh = ?draw.mesh.0.id(), mesh_path = ?self.server.get_path(draw.mesh.0.id()),
                mesh_load_state = ?self.server.get_load_state(draw.mesh.0.id()), mesh_loaded = loaded.is_some(),
                vertices = ?loaded.map(Mesh::count_vertices), topology = ?loaded.map(Mesh::primitive_topology),
                joint_indices = ?loaded.and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)).map(|attribute| attribute.len()),
                joint_weights = ?loaded.and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)).map(|attribute| attribute.len()),
                skin_present = draw.skin.is_some(),
                authored_material_name = ?draw.authored_material_name.map(|name| name.0.as_str()),
                first_authored_pass = ?draw.pass_order.map(|order| order.is_first_authored_pass()),
                authored_pass_index = "private ordering field; not observed",
                technique_invocation = ?draw.pass_order.and_then(|order| std::iter::once(entity).chain(self.parents.iter_ancestors(entity))
                    .take_while(|ancestor| *ancestor != root).find(|ancestor| order.belongs_to_technique_invocation(*ancestor))),
                "animal preview loaded draw geometry and authored pass ownership");
            let mut hierarchy_count = 0_usize;
            for ancestor in std::iter::once(entity).chain(self.parents.iter_ancestors(entity)) {
                hierarchy_count += 1;
                if hierarchy_count <= HIERARCHY_SAMPLE_LIMIT {
                    self.log_node(ancestor);
                }
                if ancestor == root {
                    break;
                }
            }
            info!(target: "openzt2_verification", hierarchy_count,
                hierarchy_truncated = hierarchy_count.saturating_sub(HIERARCHY_SAMPLE_LIMIT), "animal preview draw hierarchy sample limit");
            for camera in self
                .cameras
                .iter()
                .filter(|camera| self.matching_camera(camera, root, portrait, session))
                .take(CAMERA_SAMPLE_LIMIT)
            {
                info!(target: "openzt2_verification", camera = ?camera.entity, active = camera.camera.is_active,
                    mesh_listed = ?camera.visible.map(|visible| visible.get(TypeId::of::<Mesh3d>()).contains(&entity)),
                    "animal preview per-camera draw association");
            }
            self.log_materials(&draw);
            self.log_tint_projection(entity, &draw, loaded);
        }
        info!(target: "openzt2_verification", model_count, draw_count, excluded_draw_count, camera_count,
            models_truncated = model_count.saturating_sub(MESH_SAMPLE_LIMIT), draws_truncated = draw_count.saturating_sub(MESH_SAMPLE_LIMIT),
            cameras_truncated = camera_count.saturating_sub(CAMERA_SAMPLE_LIMIT), ?skin_totals,
            "animal preview subject totals and detailed sample limits");
    }

    fn log_model(&self, entity: Entity, model: &PreviewModelObservationItem<'_, '_>) {
        info!(target: "openzt2_verification", ?entity, model = ?model.model.model, path = %model.model.model_path,
            handle = ?model.model.handle.as_ref().map(Handle::id),
            load_state = ?model.model.handle.as_ref().and_then(|handle| self.server.get_load_state(handle.id())),
            owner = ?model.owner.map(|owner| owner.0), attachment_resolved = model.resolved,
            animal_owner = ?model.animal_owner.map(|owner| owner.0),
            controller_present = model.controller.is_some(), player_present = model.player.is_some(),
            graph = ?model.graph.map(|handle| handle.0.id()),
            pending_sets = ?model.pending.map(|pending| pending.pending_model_animation_set_asset_handles.len()),
            pending_initial_clip = ?model.pending.and_then(|pending| pending.requested_initial_animation_clip_asset_key.as_deref()),
            "animal preview model and animation attachment");
        self.log_node(entity);
        if let Some(pending) = model.pending {
            for handle in pending
                .pending_model_animation_set_asset_handles
                .iter()
                .take(MESH_SAMPLE_LIMIT)
            {
                info!(target: "openzt2_verification", ?entity, set = ?handle.id(), path = ?self.server.get_path(handle.id()),
                    load_state = ?self.server.get_load_state(handle.id()), "animal preview pending animation set");
            }
            info!(target: "openzt2_verification", ?entity,
                pending_sets_truncated = pending.pending_model_animation_set_asset_handles.len().saturating_sub(MESH_SAMPLE_LIMIT),
                "animal preview pending set sample limit");
        }
        if let Some(controller) = model.controller {
            let set = self
                .animation_sets
                .as_ref()
                .and_then(|sets| sets.get(&controller.animation_set_asset));
            let clip = set.and_then(|set| {
                set.find_animation_clip_record_by_asset_key(&controller.animation_clip_asset_key)
            });
            info!(target: "openzt2_verification", ?entity, set = ?controller.animation_set_asset.id(),
                set_path = ?self.server.get_path(controller.animation_set_asset.id()), set_loaded = set.is_some(),
                set_load_state = ?self.server.get_load_state(controller.animation_set_asset.id()),
                model_path = ?set.map(|set| set.model_asset_path.as_str()), skeleton_path = ?set.map(|set| set.skeleton_asset_path.as_str()),
                clip_key = %controller.animation_clip_asset_key, clip = ?clip.map(|clip| clip.animation_clip_handle.id()),
                clip_path = ?clip.and_then(|clip| self.server.get_path(clip.animation_clip_handle.id())),
                clip_loaded = ?clip.map(|clip| self.animation_clips.as_ref().is_some_and(|clips| clips.get(&clip.animation_clip_handle).is_some())),
                clip_load_state = ?clip.and_then(|clip| self.server.get_load_state(clip.animation_clip_handle.id())),
                duration_milliseconds = ?clip.map(|clip| clip.duration_milliseconds), elapsed_milliseconds = controller.elapsed_milliseconds,
                speed_permille = controller.playback_speed_permille, state = ?controller.playback_state,
                clock = ?controller.playback_clock, repetition = ?controller.playback_repetition_policy,
                generation = controller.playback_generation, "animal preview controller pose clock");
        }
        if let Some(player) = model.player {
            let graph = model.graph.and_then(|handle| {
                self.animation_graphs
                    .as_ref()
                    .and_then(|graphs| graphs.get(&handle.0))
            });
            let requested_clip = model.controller.and_then(|controller| {
                self.animation_sets
                    .as_ref()
                    .and_then(|sets| sets.get(&controller.animation_set_asset))
                    .and_then(|set| {
                        set.find_animation_clip_record_by_asset_key(
                            &controller.animation_clip_asset_key,
                        )
                    })
            });
            for (node, animation) in player.playing_animations().take(MESH_SAMPLE_LIMIT) {
                let graph_node = graph.and_then(|graph| graph.get(*node));
                let active_clip = graph_node.and_then(|node| match &node.node_type {
                    AnimationNodeType::Clip(clip) => Some(clip),
                    _ => None,
                });
                info!(target: "openzt2_verification", ?entity, ?node, seek_seconds = animation.seek_time(),
                    elapsed_seconds = animation.elapsed(), speed = animation.speed(), paused = animation.is_paused(), weight = animation.weight(),
                    finite = animation.seek_time().is_finite() && animation.elapsed().is_finite() && animation.speed().is_finite() && animation.weight().is_finite(),
                    graph_loaded = graph.is_some(), graph_node_present = graph_node.is_some(),
                    graph_node_type = ?graph_node.map(|node| &node.node_type),
                    graph_node_weight = ?graph_node.map(|node| node.weight), graph_node_mask = ?graph_node.map(|node| node.mask),
                    graph_node_weight_finite = ?graph_node.map(|node| node.weight.is_finite()),
                    active_clip = ?active_clip.map(Handle::id), active_clip_path = ?active_clip.and_then(|clip| self.server.get_path(clip.id())),
                    active_clip_loaded = ?active_clip.map(|clip| self.animation_clips.as_ref().is_some_and(|clips| clips.get(clip).is_some())),
                    active_clip_matches_controller = ?active_clip.zip(requested_clip).map(|(actual, requested)| actual.id() == requested.animation_clip_handle.id()),
                    "animal preview Bevy sampled animation state");
            }
            let active_count = player.playing_animations().count();
            info!(target: "openzt2_verification", ?entity, active_count,
                active_truncated = active_count.saturating_sub(MESH_SAMPLE_LIMIT), "animal preview active animation sample limit");
        }
    }
}
