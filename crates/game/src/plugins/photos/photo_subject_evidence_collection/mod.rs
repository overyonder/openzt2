use avian3d::prelude::{ColliderAabb, RigidBodyColliders, SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::system::SystemParam, prelude::*};

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::extinct_animals::fossil_collection_and_assembly_types::FossilSetAssembly;

use crate::{
    assets::animation::animation_set_asset_types::AnimationSetAsset,
    plugins::{
        animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy,
        animal_health::types::{Disease, Vitality},
        animal_lifecycle::types::{
            AnimalLifeStage, AnimalVariant, EndangeredSpecies, Parents, SpeciesHandle,
        },
        animal_welfare::types::{
            EnvironmentNeed, ExerciseNeed, HealthNeed, Hunger, HygieneNeed, PrivacyNeed, RestNeed,
            SocialNeed, StimulationNeed, Thirst,
        },
        animation_graph::{
            animation_graph_playback_state_types::AnimationGraphPlaybackState,
            animation_presentation_relationship_types::AnimationPresentationOwner,
        },
        animation_playback::animation_playback_controller_types::AnimationPlaybackController,
        aquatic::aquatic_simulation_types::AquaticAnimal,
        behavior_task_execution_types::{BehaviorTaskExecutionPhase, BehaviorTaskExecutionState},
        extinct_animals::extinct_animal_observable_fact_types::{
            AuthoredSuperExtinctAnimal, FossilEducationProgress,
        },
        guests::guest_simulation_types::GuestArchetype,
        habitat::habitat_types::{Containment, HabitatClimate, HabitatMember},
        information::entity_selection_types::Inspectable,
        world_spawn::world_membership_types::DefinitionId,
    },
};

use super::photo_capture_types::{
    PendingPhotoCapture, PendingPhotoEvidence, PendingPhotoSemantics, PhotoEvidence,
    PhotoSemanticEvidence, PhotoSubjectContainedEntitiesEvidence,
    PhotoSubjectContainedEntityEvidence, PhotoSubjectFacts, PhotoSubjectTaskEvidence,
    PhotoSubjectsCollected, PhotographableFossil, PhotographableHealth, PhotographableShowTrick,
};

#[derive(SystemParam)]
pub(super) struct PhotographableSemanticFactQueries<'w, 's> {
    animations: Query<
        'w,
        's,
        (
            &'static AnimationPresentationOwner,
            &'static AnimationGraphPlaybackState,
            &'static AnimationPlaybackController,
        ),
    >,
    habitat_members: Query<'w, 's, &'static HabitatMember>,
    habitats: Query<'w, 's, &'static HabitatClimate>,
    aquatic: Query<'w, 's, (), With<AquaticAnimal>>,
    show_tricks: Query<'w, 's, &'static PhotographableShowTrick>,
    fossils: Query<'w, 's, &'static FossilSetAssembly>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    world_definitions: Res<'w, WorldDefinitions>,
    fossil_levels: Query<'w, 's, &'static FossilEducationProgress>,
    health: Query<'w, 's, (&'static Vitality, Has<Disease>, Has<EndangeredSpecies>)>,
    life_stages: Query<'w, 's, &'static AnimalLifeStage>,
    animal_variants: Query<'w, 's, &'static AnimalVariant>,
    parents: Query<'w, 's, &'static Parents>,
    needs: Query<
        'w,
        's,
        (
            Option<&'static Hunger>,
            Option<&'static Thirst>,
            Option<&'static RestNeed>,
            Option<&'static PrivacyNeed>,
            Option<&'static SocialNeed>,
            Option<&'static ExerciseNeed>,
            Option<&'static StimulationNeed>,
            Option<&'static EnvironmentNeed>,
            Option<&'static HealthNeed>,
            Option<&'static HygieneNeed>,
        ),
    >,
    authored_super_extinct_animals: Query<'w, 's, (), With<AuthoredSuperExtinctAnimal>>,
    names: Query<'w, 's, (), With<Name>>,
    containment: Query<'w, 's, &'static Containment>,
    entity_kinds: Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static GuestArchetype>,
            Option<&'static DefinitionId>,
            Option<&'static Inspectable>,
        ),
    >,
}

pub(super) fn collect_visible_photo_subjects_and_semantic_evidence_for_pending_captures(
    spatial_query: SpatialQuery,
    mut commands: Commands,
    mut cameras: Query<
        (
            Entity,
            &Camera,
            &GlobalTransform,
            &mut PendingPhotoCapture,
            &mut PendingPhotoEvidence,
            &mut PendingPhotoSemantics,
        ),
        Without<PhotoSubjectsCollected>,
    >,
    subjects: Query<
        (
            Entity,
            Option<&SpeciesHandle>,
            Option<&DefinitionId>,
            &GlobalTransform,
            &InheritedVisibility,
            Option<&RigidBodyColliders>,
            Option<&BehaviorTaskExecutionState>,
        ),
        Or<(With<SpeciesHandle>, With<DefinitionId>)>,
    >,
    collider_bounds: Query<&ColliderAabb>,
    animation_set_assets: Res<Assets<AnimationSetAsset>>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    occupancy: Res<InteractionContainerOccupancy>,
    semantic_queries: PhotographableSemanticFactQueries,
) {
    for (camera_entity, camera, camera_transform, mut pending, mut evidence, mut semantics) in
        &mut cameras
    {
        // Include stationary objects as well as moving animals.
        for (entity, species, definition, transform, visible, colliders, behavior_task) in &subjects
        {
            let Some(definition) = species
                .map(|species| species.species)
                .or_else(|| definition.map(|definition| definition.0))
            else {
                continue;
            };
            let Some(coverage) = colliders.and_then(|colliders| {
                calculate_collision_bounds_screen_coverage_for_photo_camera(
                    camera,
                    camera_transform,
                    colliders,
                    &collider_bounds,
                )
            }) else {
                continue;
            };
            if !visible.get() || coverage.screen_permille == 0 {
                continue;
            }
            let facts = calculate_photo_subject_visual_facts(
                camera,
                camera_transform,
                transform,
                colliders.expect("coverage requires colliders"),
                coverage,
                &collider_bounds,
                &spatial_query,
                behavior_task.is_some(),
            );
            pending.subjects.insert(entity);
            evidence.insert(PhotoEvidence {
                entity,
                definition,
                facts,
            });
            semantics.0.push(PhotoSemanticEvidence {
                entity,
                position_mm: (transform.translation() * 1000.0).round().as_ivec3(),
                animal_variant: semantic_queries
                    .animal_variants
                    .get(entity)
                    .ok()
                    .map(|variant| variant.0),
                behavior: behavior_task.map(BehaviorTaskExecutionState::program),
                animation: semantic_queries.animations.iter().find_map(
                    |(owner, state, animation_playback_controller)| {
                        (owner.gameplay_entity == entity)
                            .then(|| {
                                animation_set_assets
                                    .get(&animation_playback_controller.animation_set_asset)
                            })
                            .flatten()
                            .and_then(|asset| {
                                asset
                                    .animation_graph_nodes
                                    .get(state.current_animation_graph_node_index as usize)
                            })
                            .map(|animation_graph_node| {
                                animation_graph_node.animation_graph_node_asset_key.clone()
                            })
                    },
                ),
                object_use: behavior_task
                    .and_then(BehaviorTaskExecutionState::target)
                    .and_then(|target| semantic_queries.entity_kinds.get(target).ok())
                    .and_then(|(species, _, definition, _)| {
                        definition
                            .map(|definition| definition.0)
                            .or_else(|| species.map(|species| species.species))
                    }),
                habitat: semantic_queries
                    .habitat_members
                    .get(entity)
                    .ok()
                    .and_then(|member| semantic_queries.habitats.get(member.habitat_entity).ok())
                    .map(|climate| climate.biome_identifier),
                aquatic: semantic_queries.aquatic.contains(entity),
                show_trick: semantic_queries
                    .show_tricks
                    .get(entity)
                    .ok()
                    .map(|fact| fact.0),
                fossil: semantic_queries
                    .fossils
                    .get(entity)
                    .ok()
                    .and_then(|assembly| {
                        let definitions = semantic_queries
                            .world_definitions
                            .get(&semantic_queries.world_definition_assets)?;
                        let set = definitions.find_fossil_set(assembly.fossil_set_identifier())?;
                        Some(PhotographableFossil {
                            set: assembly.fossil_set_identifier(),
                            complete: usize::from(assembly.placed_fossil_piece_count())
                                == set.pieces.len(),
                        })
                    }),
                fossil_bone_level: semantic_queries
                    .fossil_levels
                    .get(entity)
                    .ok()
                    .map(|progress| progress.bone_level.round() as u16),
                health: semantic_queries.health.get(entity).ok().map(
                    |(vitality, diseased, endangered)| PhotographableHealth {
                        health_milli: (vitality.0 * 1000.0).round() as i32,
                        diseased,
                        endangered,
                    },
                ),
                life_stage: semantic_queries
                    .life_stages
                    .get(entity)
                    .ok()
                    .map(|stage| stage.0),
                mother: semantic_queries
                    .parents
                    .get(entity)
                    .ok()
                    .and_then(|parents| parents.mother),
                father: semantic_queries
                    .parents
                    .get(entity)
                    .ok()
                    .and_then(|parents| parents.father),
                needs_q16: semantic_queries
                    .needs
                    .get(entity)
                    .map_or([None; 10], |needs| {
                        [
                            needs.0.map(|need| need.0),
                            needs.1.map(|need| need.0),
                            needs.2.map(|need| need.0),
                            needs.3.map(|need| need.0),
                            needs.4.map(|need| need.0),
                            needs.5.map(|need| need.0),
                            needs.6.map(|need| need.0),
                            needs.7.map(|need| need.0),
                            needs.8.map(|need| need.0),
                            needs.9.map(|need| need.0),
                        ]
                    }),
                super_animal: semantic_queries
                    .authored_super_extinct_animals
                    .contains(entity),
                named: semantic_queries.names.contains(entity),
                contained: semantic_queries
                    .containment
                    .get(entity)
                    .is_ok_and(|fact| fact.is_contained),
                task: capture_photo_subject_task_evidence(
                    behavior_task,
                    &behavior_document_assets,
                    &semantic_queries.entity_kinds,
                ),
                contained_entities: capture_photo_subject_contained_entities(
                    entity,
                    &occupancy,
                    &semantic_queries.entity_kinds,
                ),
            });
        }
        commands
            .entity(camera_entity)
            .insert(PhotoSubjectsCollected);
    }
}

/// Records docked occupants, excluding queue waiters.
fn capture_photo_subject_contained_entities<'w, 's>(
    container: Entity,
    occupancy: &InteractionContainerOccupancy,
    entity_kinds: &Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static GuestArchetype>,
            Option<&'static DefinitionId>,
            Option<&'static Inspectable>,
        ),
    >,
) -> PhotoSubjectContainedEntitiesEvidence {
    let mut contained = Vec::new();
    for entity in occupancy.docked_member_entities_in_container(container) {
        let Ok((species, guest, definition, inspectable)) = entity_kinds.get(entity) else {
            return PhotoSubjectContainedEntitiesEvidence::UnsupportedAtCapture;
        };
        let Some(definition) =
            resolve_photo_subject_contained_entity_kind(species, guest, definition, inspectable)
        else {
            return PhotoSubjectContainedEntitiesEvidence::UnsupportedAtCapture;
        };
        contained.push(PhotoSubjectContainedEntityEvidence { entity, definition });
    }
    if contained.is_empty() {
        PhotoSubjectContainedEntitiesEvidence::Empty
    } else {
        PhotoSubjectContainedEntitiesEvidence::Contained(contained)
    }
}

fn resolve_photo_subject_contained_entity_kind(
    species: Option<&SpeciesHandle>,
    guest: Option<&GuestArchetype>,
    definition: Option<&DefinitionId>,
    inspectable: Option<&Inspectable>,
) -> Option<openzt2_game_data::AssetId> {
    species
        .map(|species| species.species)
        .or_else(|| guest.map(|guest| guest.0))
        .or_else(|| definition.map(|definition| definition.0))
        .or_else(|| inspectable.map(|inspectable| inspectable.definition))
}

/// Resolves the current task and target while their live definitions are available.
fn capture_photo_subject_task_evidence<'w, 's>(
    behavior_task: Option<&BehaviorTaskExecutionState>,
    behavior_document_assets: &Assets<BehaviorDocumentAsset>,
    entity_kinds: &Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static GuestArchetype>,
            Option<&'static DefinitionId>,
            Option<&'static Inspectable>,
        ),
    >,
) -> PhotoSubjectTaskEvidence {
    let Some(behavior_task) = behavior_task else {
        return PhotoSubjectTaskEvidence::NoCurrentTask;
    };
    // Resolve the nearest non-Set frame, as reservation_tag does. A missing
    // declaration must not fall through to an older task.
    let Some(selected_task) = select_nearest_non_set_behavior_task_frame(behavior_task) else {
        // A standalone behavior set has no enclosing task name.
        return PhotoSubjectTaskEvidence::UnresolvedAtCapture;
    };
    let Some(task_document_asset) = behavior_document_assets.get(selected_task.document) else {
        return PhotoSubjectTaskEvidence::UnresolvedAtCapture;
    };
    let Some(authored_task_name) = task_document_asset
        .behavior_task_at_index(selected_task.declaration)
        .map(|declaration| declaration.name.trim().to_string())
    else {
        return PhotoSubjectTaskEvidence::UnresolvedAtCapture;
    };
    capture_photo_subject_task_evidence_from_resolved_name(
        selected_task.target,
        authored_task_name,
        entity_kinds,
    )
}

struct SelectedBehaviorTaskFrame<'a> {
    document: &'a Handle<BehaviorDocumentAsset>,
    declaration: usize,
    /// Return frames do not carry a separate target; nested sets inherit the
    /// execution state's target, so selection keeps that same task ownership.
    target: Option<Entity>,
}

fn select_nearest_non_set_behavior_task_frame(
    behavior_task: &BehaviorTaskExecutionState,
) -> Option<SelectedBehaviorTaskFrame<'_>> {
    std::iter::once((
        &behavior_task.document,
        behavior_task.declaration,
        behavior_task.phase,
    ))
    .chain(
        behavior_task
            .stack
            .iter()
            .rev()
            .map(|frame| (&frame.document, frame.declaration, frame.phase)),
    )
    .find_map(|(document, declaration, phase)| {
        (phase != BehaviorTaskExecutionPhase::Set).then_some(SelectedBehaviorTaskFrame {
            document,
            declaration,
            target: behavior_task.target,
        })
    })
}

fn capture_photo_subject_task_evidence_from_resolved_name<'w, 's>(
    target: Option<Entity>,
    authored_task_name: String,
    entity_kinds: &Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static GuestArchetype>,
            Option<&'static DefinitionId>,
            Option<&'static Inspectable>,
        ),
    >,
) -> PhotoSubjectTaskEvidence {
    // A missing target definition leaves the task evidence unresolved.
    let target_definition = target.and_then(|target| {
        entity_kinds
            .get(target)
            .ok()
            .and_then(|(species, guest, definition, inspectable)| {
                resolve_photo_subject_contained_entity_kind(species, guest, definition, inspectable)
            })
    });
    if target.is_some() && target_definition.is_none() {
        return PhotoSubjectTaskEvidence::UnresolvedAtCapture;
    }
    PhotoSubjectTaskEvidence::CurrentTask {
        authored_task_name,
        target_definition,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CollisionScreenCoverage {
    screen_permille: u16,
    viewport_visible_permille: u16,
    world_center: Vec3,
}

/// Projects each collider's minimum and maximum corner and clips their union
/// to the viewport. Screen coverage and the visible fraction of the subject
/// are separate scoring inputs.
fn calculate_collision_bounds_screen_coverage_for_photo_camera(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    colliders: &RigidBodyColliders,
    bounds: &Query<&ColliderAabb>,
) -> Option<CollisionScreenCoverage> {
    let mut ndc_min = Vec2::splat(f32::INFINITY);
    let mut ndc_max = Vec2::splat(f32::NEG_INFINITY);
    let mut world_min = Vec3::splat(f32::INFINITY);
    let mut world_max = Vec3::splat(f32::NEG_INFINITY);

    for collider in colliders {
        let bounds = bounds.get(collider).ok()?;
        if !bounds.min.is_finite() || !bounds.max.is_finite() {
            return None;
        }
        let first = camera.world_to_ndc(camera_transform, bounds.min)?;
        let second = camera.world_to_ndc(camera_transform, bounds.max)?;
        if !(0.0..=1.0).contains(&first.z) || !(0.0..=1.0).contains(&second.z) {
            return None;
        }
        ndc_min = ndc_min.min(first.truncate().min(second.truncate()));
        ndc_max = ndc_max.max(first.truncate().max(second.truncate()));
        world_min = world_min.min(bounds.min);
        world_max = world_max.max(bounds.max);
    }

    if !ndc_min.is_finite() || !ndc_max.is_finite() {
        return None;
    }
    let clipped_min = ndc_min.max(Vec2::NEG_ONE).min(Vec2::ONE);
    let clipped_max = ndc_max.max(Vec2::NEG_ONE).min(Vec2::ONE);
    let clipped_extent = (clipped_max - clipped_min).max(Vec2::ZERO);
    let screen_area = clipped_extent.element_product() * 0.25;
    let projected_extent = (ndc_max - ndc_min).max(Vec2::ZERO);
    let projected_area = projected_extent.element_product();

    Some(CollisionScreenCoverage {
        screen_permille: convert_fraction_to_rounded_permille(screen_area),
        viewport_visible_permille: convert_fraction_to_rounded_permille(
            (projected_area > 0.0)
                .then(|| clipped_extent.element_product() / projected_area)
                .unwrap_or(0.0),
        ),
        world_center: world_min.midpoint(world_max),
    })
}

fn calculate_photo_subject_visual_facts(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    subject_transform: &GlobalTransform,
    colliders: &RigidBodyColliders,
    coverage: CollisionScreenCoverage,
    bounds: &Query<&ColliderAabb>,
    spatial_query: &SpatialQuery,
    has_behavior: bool,
) -> PhotoSubjectFacts {
    let center_permille = camera
        .world_to_ndc(camera_transform, coverage.world_center)
        .map(|ndc| 1.0 - ndc.truncate().length() / Vec2::ONE.length())
        .map_or(0, convert_fraction_to_rounded_permille);
    let toward_camera =
        (camera_transform.translation() - coverage.world_center).normalize_or_zero();
    let facing_permille = convert_fraction_to_rounded_permille(
        (subject_transform.forward().as_vec3().dot(toward_camera) + 1.0) * 0.5,
    );
    PhotoSubjectFacts {
        screen_permille: coverage.screen_permille,
        viewport_visible_permille: coverage.viewport_visible_permille,
        center_permille,
        facing_permille,
        behavior_permille: if has_behavior { 1000 } else { 0 },
        unoccluded: photo_subject_collision_bounds_are_unoccluded_from_camera(
            camera_transform.translation(),
            coverage.world_center,
            colliders,
            bounds,
            spatial_query,
        ),
    }
}

fn photo_subject_collision_bounds_are_unoccluded_from_camera(
    origin: Vec3,
    target: Vec3,
    colliders: &RigidBodyColliders,
    bounds: &Query<&ColliderAabb>,
    spatial_query: &SpatialQuery,
) -> bool {
    let displacement = target - origin;
    let distance = displacement.length();
    let Ok(direction) = Dir3::new(displacement) else {
        return false;
    };
    spatial_query
        .cast_ray(
            origin,
            direction,
            distance + 0.001,
            false,
            &SpatialQueryFilter::DEFAULT,
        )
        .is_some_and(|hit| {
            colliders.into_iter().any(|collider| {
                collider == hit.entity
                    && bounds
                        .get(collider)
                        .is_ok_and(|aabb| aabb.min.is_finite() && aabb.max.is_finite())
            })
        })
}

#[inline]
fn convert_fraction_to_rounded_permille(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 1000.0).round() as u16
}
