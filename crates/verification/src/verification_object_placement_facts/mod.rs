use std::any::TypeId;

use bevy::{
    animation::AnimatedBy,
    camera::visibility::VisibleEntities,
    ecs::{query::QueryData, system::SystemParam},
    mesh::skinning::SkinnedMesh,
    prelude::*,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectContainerContent, WorldObjectDefinition, WorldObjectKind,
};

use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationSetAsset,
    source_document::path::AssetPath,
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::ModelAnimationAttachmentResolved;
use openzt2_world_state::animation_graph::model_animation_asset_binding_types::PendingModelAnimationAssets;
use openzt2_world_state::animation_playback::animation_joint_target_types::AnimationJointTarget;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::DomainAckStatus;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::feeding::container_quantity::AUTHORED_CONTAINER_CAPACITY_Q16;
use openzt2_zoo_state::habitat::habitat_types::Containment;
use openzt2_zoo_state::habitat::habitat_types::Habitat;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewOwner;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRenderable;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationFacts, VerificationJourneyReport,
    VerificationJourneySession,
};

#[derive(QueryData)]
struct VerificationPlacedObject {
    entity: Entity,
    placed: &'static PlacedObjectDefinitionReference,
    definition: Option<&'static DefinitionId>,
    persistent: Option<&'static PersistentId>,
    member: Option<&'static WorldMember>,
    habitat: Option<&'static HabitatMember>,
    containment: Option<&'static Containment>,
    food: Option<&'static FoodContainer>,
    drink: Option<&'static DrinkContainer>,
}

#[derive(QueryData)]
struct VerificationAnimalPreviewModel {
    entity: Entity,
    owner: &'static ObjectPlacementPreviewOwner,
    model: Option<&'static PrefabModel>,
    pending: Option<&'static PendingModelAnimationAssets>,
    resolved: Option<&'static ModelAnimationAttachmentResolved>,
    controller: Option<&'static AnimationPlaybackController>,
    player: Option<&'static AnimationPlayer>,
}

#[derive(Default)]
struct VerificationAnimalPreviewStageCounts {
    canonical_definitions: u32,
    renderables: u32,
    models: u32,
    loaded_models: u32,
    failed_models: u32,
    pending_animation_models: u32,
    resolved_animation_models: u32,
    loaded_animation_sets: u32,
    matching_animation_sets: u32,
    failed_animation_sets: u32,
    pending_clips: u32,
    loaded_clips: u32,
    failed_clips: u32,
    controllers: u32,
    players: u32,
    playing_models: u32,
    skinned_meshes: u32,
    loaded_skinned_meshes: u32,
    bound_skinned_meshes: u32,
    camera_visible_skinned_meshes: u32,
    animated_visible_meshes: u32,
}

#[derive(SystemParam)]
pub(super) struct VerificationObjectPlacementQueries<'w, 's> {
    definitions: Res<'w, Assets<WorldDefinitionAsset>>,
    active_definitions: Res<'w, WorldDefinitions>,
    roots: Query<'w, 's, Entity, With<WorldRoot>>,
    selected: Res<'w, SelectedEntity>,
    transforms: Query<'w, 's, &'static Transform>,
    relocating: Query<'w, 's, (), With<RelocatingPlacedObject>>,
    relocation_previews: Query<
        'w,
        's,
        (
            Entity,
            &'static PlacedObjectRelocationSource,
            &'static ConstructionPreview,
        ),
    >,
    completed_edits: Query<
        'w,
        's,
        (
            &'static PreparedObjectPlacementEdit,
            &'static EditCommitProgress,
        ),
    >,
    model_handles: Query<'w, 's, &'static PrefabModel>,
    render_meshes: Query<'w, 's, &'static Mesh3d>,
    habitats: Query<'w, 's, &'static WorldMember, With<Habitat>>,
    objects: Query<'w, 's, VerificationPlacedObject>,
    edits: Query<'w, 's, &'static PreparedObjectPlacementEdit>,
    previews: Query<'w, 's, &'static ConstructionPreview>,
    preview_models:
        Query<'w, 's, VerificationAnimalPreviewModel, With<ObjectPlacementPreviewRenderable>>,
    cameras: Query<'w, 's, (&'static Camera, &'static VisibleEntities), With<ZooCamera>>,
    children: Query<'w, 's, &'static Children>,
    skinned_meshes: Query<'w, 's, (&'static Mesh3d, &'static SkinnedMesh)>,
    animation_joints: Query<'w, 's, (&'static AnimationJointTarget, &'static AnimatedBy)>,
    meshes: Res<'w, Assets<Mesh>>,
    server: Res<'w, AssetServer>,
    species_assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    species_index: Option<Res<'w, SpeciesAssets>>,
    animation_sets: Res<'w, Assets<AnimationSetAsset>>,
    animation_clips: Res<'w, Assets<AnimationClip>>,
}

pub(super) fn collect_object_placement_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    world: VerificationObjectPlacementQueries,
    mut completed: MessageReader<TransactionCompleted>,
    mut failed_commits: MessageReader<ConstructionCommitFailed>,
) {
    let report = &mut *report;
    // Construction and sale failures are otherwise silent; name each reason.
    for failure in failed_commits.read() {
        report.construction_commit_failures = report.construction_commit_failures.saturating_add(1);
        info!(target: "openzt2_verification", reason = ?failure.reason, "construction commit failed");
    }
    // Read every frame: settlement messages expire between journey snapshots.
    // Keep actual transfers, including duplicates and compensation, for this journey.
    report.construction_payments.extend(
        completed
            .read()
            .filter(|payment| payment.kind == TransactionKind::Construction)
            .copied(),
    );
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    report.current_facts.insert(
        "construction_payments".to_owned(),
        f64::from(u32::try_from(report.construction_payments.len()).unwrap_or(u32::MAX)),
    );
    report.current_facts.insert(
        "construction_commit_failures".to_owned(),
        f64::from(report.construction_commit_failures),
    );
    collect_single_object_sale_verification_facts(
        &world,
        &report.construction_payments,
        &mut report.current_facts,
    );
    collect_animal_preview_renderer_verification_facts(&world, &mut report.current_facts);
    collect_selected_object_relocation_facts(&world, &mut report.current_facts);
    for (name, key) in [
        ("placed_burger_stands", "foodstand_burger_df"),
        ("placed_gift_carts", "giftcart_df"),
    ] {
        let definition = openzt2_game_data::AssetId::from_key(key);
        let count = world
            .objects
            .iter()
            .filter(|object| object.placed.definition == definition)
            .count();
        report.current_facts.insert(
            name.to_owned(),
            f64::from(u32::try_from(count).unwrap_or(u32::MAX)),
        );
    }
    for (prefix, kind) in [
        ("food", WorldObjectKind::Food),
        ("enrichment", WorldObjectKind::Enrichment),
    ] {
        collect_catalogue_category_placement_facts(
            &world,
            &report.construction_payments,
            kind,
            prefix,
            &mut report.current_facts,
        );
    }
}

fn collect_selected_object_relocation_facts(
    world: &VerificationObjectPlacementQueries,
    facts: &mut VerificationFacts,
) {
    let selected = world
        .selected
        .0
        .and_then(|entity| world.objects.get(entity).ok());
    let mut id = 0_u64;
    let mut position = Vec3::ZERO;
    let mut visible_meshes = 0;
    let mut loaded_models = 0;
    let mut relocating = false;
    let mut matching_previews = 0;
    let mut preview_visible_meshes = 0;
    let mut destination = Vec3::ZERO;
    let mut completed_relocations = 0;
    let mut identity_instances = 0;
    if let Some(object) = selected {
        id = object.persistent.map_or(0, |id| id.0);
        identity_instances = world
            .objects
            .iter()
            .filter(|candidate| id != 0 && candidate.persistent == object.persistent)
            .count();
        position = world
            .transforms
            .get(object.entity)
            .map_or(Vec3::ZERO, |t| t.translation);
        relocating = world.relocating.contains(object.entity);
        visible_meshes = count_visible_model_meshes(world, object.entity);
        loaded_models = std::iter::once(object.entity)
            .chain(
                world
                    .children
                    .iter_descendants_depth_first::<Children>(object.entity),
            )
            .filter(|entity| {
                world
                    .model_handles
                    .get(*entity)
                    .is_ok_and(|model| model.handle.is_some())
            })
            .count();
        for (preview, source, state) in &world.relocation_previews {
            if source.0 == object.entity {
                matching_previews += 1;
                destination = state.transform.translation;
                preview_visible_meshes += count_visible_model_meshes(world, preview);
            }
        }
        completed_relocations = world
            .completed_edits
            .iter()
            .filter(|(edit, progress)| {
                progress.phase == EditCommitPhase::Complete
                    && progress.placement == DomainAckStatus::Applied
                    && edit.mutations.iter().any(|mutation| {
                        mutation.mutation == PreparedObjectPlacementEditMutationKind::Relocate
                            && object.persistent == Some(&mutation.entity)
                            && mutation.applied_entity == Some(object.entity)
                    })
            })
            .count();
    }
    let bytes = id.to_le_bytes();
    let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let high = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let count = |value| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
    for (name, value) in [
        ("selected_object_id_low", f64::from(low)),
        ("selected_object_id_high", f64::from(high)),
        (
            "selected_object_identity_instances",
            count(identity_instances),
        ),
        ("selected_object_x_m", f64::from(position.x)),
        ("selected_object_z_m", f64::from(position.z)),
        ("selected_object_visible_meshes", count(visible_meshes)),
        ("selected_object_loaded_models", count(loaded_models)),
        (
            "selected_object_relocating",
            f64::from(u8::from(relocating)),
        ),
        (
            "selected_object_relocation_previews",
            f64::from(matching_previews),
        ),
        (
            "relocation_preview_visible_meshes",
            count(preview_visible_meshes),
        ),
        ("relocation_destination_x_m", f64::from(destination.x)),
        ("relocation_destination_z_m", f64::from(destination.z)),
        (
            "relocation_distance_squared_m",
            f64::from((destination - position).length_squared()),
        ),
        (
            "selected_object_completed_relocations",
            count(completed_relocations),
        ),
    ] {
        facts.insert(name.to_owned(), value);
    }
}

fn count_visible_model_meshes(world: &VerificationObjectPlacementQueries, root: Entity) -> usize {
    std::iter::once(root)
        .chain(
            world
                .children
                .iter_descendants_depth_first::<Children>(root),
        )
        .filter(|entity| {
            world.render_meshes.get(*entity).is_ok_and(|mesh| {
                world
                    .meshes
                    .get(&mesh.0)
                    .is_some_and(|mesh| mesh.count_vertices() > 0)
            }) && world.cameras.iter().any(|(camera, visible)| {
                camera.is_active && visible.get(TypeId::of::<Mesh3d>()).contains(entity)
            })
        })
        .count()
}

fn collect_animal_preview_renderer_verification_facts(
    world: &VerificationObjectPlacementQueries,
    facts: &mut VerificationFacts,
) {
    let mut counts = VerificationAnimalPreviewStageCounts::default();
    let species = world
        .species_index
        .as_ref()
        .zip(world.species_assets.as_ref())
        .and_then(|(index, assets)| index.get(assets));
    if let Some(catalogue) = world.active_definitions.get(&world.definitions) {
        let is_canonical_animal_preview = |preview: &ConstructionPreview| {
            catalogue.find_object(preview.definition).is_some()
                && species.is_some_and(|species| {
                    species.find(preview.definition).is_some()
                        || species.find_variant(preview.definition).is_some()
                })
        };
        for preview in &world.previews {
            if is_canonical_animal_preview(preview) {
                counts.canonical_definitions += 1;
            }
        }
        for model in &world.preview_models {
            let is_animal_preview = world
                .previews
                .get(model.owner.0)
                .is_ok_and(&is_canonical_animal_preview);
            if !is_animal_preview {
                continue;
            }
            counts.renderables += 1;
            if let Some(prefab_model) = model.model {
                counts.models += 1;
                if let Some(handle) = &prefab_model.handle {
                    match world.server.get_load_state(handle.id()) {
                        Some(bevy::asset::LoadState::Loaded) => counts.loaded_models += 1,
                        Some(bevy::asset::LoadState::Failed(_)) => counts.failed_models += 1,
                        _ => {}
                    }
                }
            }
            counts.pending_animation_models += u32::from(model.pending.is_some());
            // Resolution also covers animationless models; controller and playing
            // observations establish whether an animation was actually attached.
            counts.resolved_animation_models += u32::from(model.resolved.is_some());
            counts.controllers += u32::from(model.controller.is_some());
            counts.players += u32::from(model.player.is_some());
            let is_playing = model
                .player
                .is_some_and(|player| player.playing_animations().next().is_some());
            counts.playing_models += u32::from(is_playing);
            count_animal_preview_animation_load_stages(world, &model, &mut counts);
            count_animal_preview_skinned_geometry_stages(world, &model, is_playing, &mut counts);
        }
    }
    counts.insert_facts(species.is_some(), facts);
}

fn count_animal_preview_animation_load_stages(
    world: &VerificationObjectPlacementQueries,
    model: &VerificationAnimalPreviewModelItem<'_, '_>,
    counts: &mut VerificationAnimalPreviewStageCounts,
) {
    let clip_key = model
        .pending
        .and_then(|pending| {
            pending
                .requested_initial_animation_clip_asset_key
                .as_deref()
        })
        .or_else(|| {
            model
                .controller
                .map(|controller| controller.animation_clip_asset_key.as_str())
        });
    for handle in model
        .pending
        .into_iter()
        .flat_map(|pending| pending.pending_model_animation_set_asset_handles.iter())
        .chain(
            model
                .controller
                .into_iter()
                .map(|controller| &controller.animation_set_asset),
        )
    {
        if matches!(
            world.server.get_load_state(handle.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) {
            counts.failed_animation_sets += 1;
        }
        let Some(set) = world.animation_sets.get(handle) else {
            continue;
        };
        counts.loaded_animation_sets += 1;
        counts.matching_animation_sets += u32::from(model.model.is_some_and(|prefab_model| {
            !set.model_asset_path.is_empty()
                && openzt2_game_data::AssetId::from_virtual_path(
                    &AssetPath::new(&set.model_asset_path).key(),
                ) == prefab_model.model
        }));
        let Some(clip_key) = clip_key else {
            continue;
        };
        counts.pending_clips += u32::from(set.animation_clip_asset_load_is_pending(clip_key));
        counts.failed_clips += u32::from(set.animation_clip_asset_load_has_failed(clip_key));
        counts.loaded_clips += u32::from(
            set.find_animation_clip_record_by_asset_key(clip_key)
                .is_some_and(|clip| {
                    world
                        .animation_clips
                        .get(&clip.animation_clip_handle)
                        .is_some()
                }),
        );
    }
}

fn count_animal_preview_skinned_geometry_stages(
    world: &VerificationObjectPlacementQueries,
    model: &VerificationAnimalPreviewModelItem<'_, '_>,
    is_playing: bool,
    counts: &mut VerificationAnimalPreviewStageCounts,
) {
    for entity in world
        .children
        .iter_descendants_depth_first::<Children>(model.entity)
    {
        let Ok((mesh, skin)) = world.skinned_meshes.get(entity) else {
            continue;
        };
        counts.skinned_meshes += 1;
        let has_bound_animation = skin.joints.iter().any(|joint| {
            world
                .animation_joints
                .get(*joint)
                .is_ok_and(|(target, player)| {
                    target.animation_playback_controller_entity == model.entity
                        && player.0 == model.entity
                })
        });
        let is_visible_in_game_camera = world.cameras.iter().any(|(camera, visible)| {
            camera.is_active && visible.get(TypeId::of::<Mesh3d>()).contains(&entity)
        });
        let is_loaded = world
            .meshes
            .get(&mesh.0)
            .is_some_and(|mesh| mesh.count_vertices() > 0);
        if is_loaded {
            counts.loaded_skinned_meshes += 1;
            counts.bound_skinned_meshes += u32::from(has_bound_animation);
            counts.camera_visible_skinned_meshes += u32::from(is_visible_in_game_camera);
            if model.controller.is_some()
                && is_playing
                && has_bound_animation
                && is_visible_in_game_camera
            {
                counts.animated_visible_meshes += 1;
            }
        }
    }
}

impl VerificationAnimalPreviewStageCounts {
    fn insert_facts(&self, species_available: bool, facts: &mut VerificationFacts) {
        // These observations share the existing checkpoints and failure snapshot.
        // Keep the stages independent so an absent controller does not hide meshes.
        for (stage, count) in [
            ("species_available", u32::from(species_available)),
            ("canonical_definitions", self.canonical_definitions),
            ("renderables", self.renderables),
            ("models", self.models),
            ("loaded_models", self.loaded_models),
            ("failed_models", self.failed_models),
            ("pending_animation_models", self.pending_animation_models),
            ("resolved_animation_models", self.resolved_animation_models),
            ("loaded_animation_sets", self.loaded_animation_sets),
            ("matching_animation_sets", self.matching_animation_sets),
            ("failed_animation_sets", self.failed_animation_sets),
            ("pending_clips", self.pending_clips),
            ("loaded_clips", self.loaded_clips),
            ("failed_clips", self.failed_clips),
            ("controllers", self.controllers),
            ("players", self.players),
            ("playing_models", self.playing_models),
            ("skinned_meshes", self.skinned_meshes),
            ("loaded_skinned_meshes", self.loaded_skinned_meshes),
            ("bound_skinned_meshes", self.bound_skinned_meshes),
            (
                "camera_visible_skinned_meshes",
                self.camera_visible_skinned_meshes,
            ),
            ("animated_visible_meshes", self.animated_visible_meshes),
        ] {
            facts.insert(format!("animal_preview_{stage}"), f64::from(count));
        }
    }
}

fn collect_single_object_sale_verification_facts(
    world: &VerificationObjectPlacementQueries,
    payments: &[TransactionCompleted],
    facts: &mut VerificationFacts,
) {
    let Some(catalogue) = world.active_definitions.get(&world.definitions) else {
        return;
    };
    let mut sale_payments = 0_u32;
    let mut correct_sale_payments = 0_u32;
    for payment in payments {
        let Some(edit) = payment
            .subject
            .and_then(|subject| world.edits.get(subject).ok())
        else {
            continue;
        };
        let [mutation] = edit.mutations.as_ref() else {
            continue;
        };
        if mutation.mutation != PreparedObjectPlacementEditMutationKind::Remove {
            continue;
        }
        sale_payments += 1;
        let refund = catalogue.find_object(mutation.definition).and_then(
            openzt2_zoo_state::economy::object_sale_refund::calculate_authored_object_sale_refund,
        );
        let removed = mutation.applied_entity.is_none()
            && !world
                .objects
                .iter()
                .any(|object| object.persistent == Some(&mutation.entity));
        correct_sale_payments += u32::from(
            removed
                && payment.debit == Account::External
                && payment.credit == Account::Zoo
                && refund == Some(payment.amount),
        );
    }
    facts.insert("object_sale_payments".to_owned(), f64::from(sale_payments));
    facts.insert(
        "correct_object_sale_payments".to_owned(),
        f64::from(correct_sale_payments),
    );
}

fn collect_catalogue_category_placement_facts(
    world: &VerificationObjectPlacementQueries,
    payments: &[TransactionCompleted],
    kind: WorldObjectKind,
    prefix: &str,
    facts: &mut VerificationFacts,
) {
    let Some(catalogue) = world.active_definitions.get(&world.definitions) else {
        return;
    };
    let mut objects = 0_u32;
    let mut owned_objects = 0_u32;
    let mut initial_quantities = 0_u32;
    let mut filled_food_containers = 0_u32;
    let mut placement_payments = 0_u32;
    let mut correct_payments = 0_u32;
    for object in &world.objects {
        let Some(authored) = catalogue
            .find_object(object.placed.definition)
            .filter(|object| object.kind == kind)
        else {
            continue;
        };
        objects += 1;
        owned_objects += u32::from(object.has_persistent_habitat_ownership(world));
        initial_quantities += u32::from(object.has_authored_initial_quantity(authored));
        filled_food_containers += u32::from(object.food.is_some_and(|food| food.amount_q16 > 0));
        info!(target: "openzt2_verification", entity = ?object.entity, definition = ?object.placed.definition,
            persistent = ?object.persistent, member = ?object.member, habitat = ?object.habitat,
            food = ?object.food, drink = ?object.drink, "committed habitat object");
    }
    for payment in payments {
        let Some(edit) = payment
            .subject
            .and_then(|subject| world.edits.get(subject).ok())
        else {
            continue;
        };
        // This journey commits one catalogue object per click. Do not let a
        // batch edit or an unrelated construction payment satisfy it.
        let [mutation] = edit.mutations.as_ref() else {
            continue;
        };
        let Some(authored) = catalogue
            .find_object(mutation.definition)
            .filter(|object| object.kind == kind)
        else {
            continue;
        };
        placement_payments += 1;
        let applied = mutation
            .applied_entity
            .and_then(|entity| world.objects.get(entity).ok());
        let correct = mutation.mutation == PreparedObjectPlacementEditMutationKind::Create
            && applied.is_some_and(|object| {
                object.placed.definition == mutation.definition
                    && object.persistent == Some(&mutation.entity)
            })
            && payment.debit == Account::Zoo
            && payment.credit == Account::External
            && payment.amount.0 > 0
            && payment.amount.0 == authored.price_cents;
        correct_payments += u32::from(correct);
        info!(target: "openzt2_verification", ?payment, expected_cents = authored.price_cents,
            "catalogue placement payment");
    }
    for (suffix, value) in [
        ("placed_objects", objects),
        ("owned_habitat_objects", owned_objects),
        ("authored_initial_quantities", initial_quantities),
        ("filled_food_containers", filled_food_containers),
        ("placement_payments", placement_payments),
        ("correct_placement_payments", correct_payments),
    ] {
        facts.insert(format!("{prefix}_{suffix}"), f64::from(value));
    }
}

impl VerificationPlacedObjectItem<'_, '_> {
    fn has_persistent_habitat_ownership(&self, world: &VerificationObjectPlacementQueries) -> bool {
        self.definition
            .is_some_and(|definition| definition.0 == self.placed.definition)
            && self.persistent.is_some_and(|id| id.0 != 0)
            && self.member.is_some_and(|member| {
                world.roots.contains(member.root)
                    && self.habitat.is_some_and(|habitat| {
                        world
                            .habitats
                            .get(habitat.habitat_entity)
                            .is_ok_and(|owner| owner.root == member.root)
                            && self.containment.is_some_and(|containment| {
                                containment.is_contained
                                    && containment.habitat_entity == habitat.habitat_entity
                            })
                    })
            })
    }

    fn has_authored_initial_quantity(&self, authored: &WorldObjectDefinition) -> bool {
        let Some(quantity) = authored.container_quantity else {
            return self.food.is_none() && self.drink.is_none();
        };
        let expected = quantity
            .initial_q16
            .clamp(0, AUTHORED_CONTAINER_CAPACITY_Q16);
        match quantity.content {
            WorldObjectContainerContent::Food => {
                self.food.is_some_and(|food| {
                    food.food == self.placed.definition
                        && food.amount_q16 == expected
                        && food.capacity_q16 == AUTHORED_CONTAINER_CAPACITY_Q16
                }) && self.drink.is_none()
            }
            WorldObjectContainerContent::Drink => {
                self.drink.is_some_and(|drink| {
                    drink.drink == self.placed.definition
                        && drink.amount_q16 == expected
                        && drink.capacity_q16 == AUTHORED_CONTAINER_CAPACITY_Q16
                }) && self.food.is_none()
            }
        }
    }
}
