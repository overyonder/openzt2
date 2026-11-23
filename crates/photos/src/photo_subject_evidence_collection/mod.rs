use avian3d::prelude::{ColliderAabb, RigidBodyColliders, SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationFrames;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{WorldDefinitions, WorldDefinitionsView};
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{FossilAssemblySlots, FossilAssemblySlotOccupant, FossilSetAssembly};
use bevy::ecs::relationship::RelationshipTarget;

use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::photos::photo_capture_types::{
    AlbumPhotoReadbackOf, PhotoMode, PhotoView,
};
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::animal_behavior::animal_lineage::AnimalLineage;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Vitality;
use openzt2_zoo_state::animal_lifecycle::types::AnimalLifeStage;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::animal_lifecycle::types::EndangeredSpecies;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_welfare::types::EnvironmentNeed;
use openzt2_zoo_state::animal_welfare::types::ExerciseNeed;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::PrivacyNeed;
use openzt2_zoo_state::animal_welfare::types::RestNeed;
use openzt2_zoo_state::animal_welfare::types::SocialNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::extinct_animals::extinct_animal_observable_fact_types::AuthoredSuperExtinctAnimal;
use openzt2_zoo_state::extinct_animals::extinct_animal_observable_fact_types::FossilEducationProgress;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::habitat::habitat_types::Containment;
use openzt2_zoo_state::habitat::habitat_types::HabitatClimate;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::topology::topology_graph_types::{
    PathTile, TileSurface, TopologyGrid, TopologyIndex,
};

use super::photo_capture_types::{
    PendingPhotoCapture, PendingPhotoEvidence, PhotoEvidence, PhotoSemanticEvidence,
    PhotoSubjectContainedEntitiesEvidence, PhotoSubjectContainedEntityEvidence, PhotoSubjectFacts,
    PhotoSubjectTaskEvidence, PhotoSubjectsCollected, PhotographableFossil, PhotographableHealth,
    PhotographableShowTrick,
};
use super::photo_configuration::{PhotoConfiguration, PhotoConfigurationSource};

#[derive(SystemParam)]
pub(super) struct PhotographableSemanticFactQueries<'w, 's> {
    continuations: BehaviorContinuationFrames<'w, 's>,
    animations: Query<
        'w,
        's,
        (
            &'static AnimationGraphPlaybackState,
            &'static AnimationPlaybackController,
        ),
    >,
    animation_presentations: Query<'w, 's, &'static AnimationPresentations>,
    habitat_members: Query<'w, 's, &'static HabitatMember>,
    habitats: Query<'w, 's, &'static HabitatClimate>,
    surfaces: DrySurfaceEligibility<'w, 's>,
    show_tricks: Query<'w, 's, &'static PhotographableShowTrick>,
    fossils: Query<'w, 's, (&'static FossilSetAssembly, &'static FossilAssemblySlots)>,
    occupied_fossil_slots: Query<'w, 's, (), With<FossilAssemblySlotOccupant>>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    world_definitions: Res<'w, WorldDefinitions>,
    fossil_levels: Query<'w, 's, &'static FossilEducationProgress>,
    health: Query<'w, 's, (&'static Vitality, Has<Disease>, Has<EndangeredSpecies>)>,
    life_stages: Query<'w, 's, &'static AnimalLifeStage>,
    animal_variants: Query<'w, 's, &'static AnimalVariant>,
    lineage: AnimalLineage<'w, 's>,
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
    names: Query<'w, 's, &'static Name>,
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
    cameras: Query<(&Camera, &GlobalTransform, &PhotoView), With<PhotoMode>>,
    mut captures: Query<
        (
            Entity,
            &AlbumPhotoReadbackOf,
            &mut PendingPhotoCapture,
            &mut PendingPhotoEvidence,
        ),
        Without<PhotoSubjectsCollected>,
    >,
    subjects: Query<
        (
            Entity,
            &WorldMember,
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
    memberships: InteractionContainerMemberships,
    semantic_queries: PhotographableSemanticFactQueries,
    topology: Option<Single<&TopologyIndex>>,
    grid: Option<Single<&TopologyGrid>>,
    path_tiles: Query<(&PathTile, &TileSurface, &GlobalTransform)>,
    configuration: Res<PhotoConfigurationSource>,
    configurations: Res<Assets<PhotoConfiguration>>,
) {
    let Some(policy) = configurations.get(&configuration.0) else {
        return;
    };
    let topology = topology.map(Single::into_inner);
    let grid = grid.map(Single::into_inner);
    for (capture_entity, owner, mut capture, mut evidence) in &mut captures {
        let Ok((camera, camera_transform, view)) = cameras.get(owner.0) else {
            continue;
        };
        capture.camera_position = camera_transform.translation();
        capture.camera_rotation = camera_transform.rotation();
        let definitions = semantic_queries
            .world_definitions
            .get(&semantic_queries.world_definition_assets);
        capture.view = PhotoView {
            elevated: definitions
                .zip(topology.as_deref())
                .zip(grid.as_deref())
                .is_some_and(|((definitions, topology), grid)| {
                    let position = capture.camera_position;
                    let widest_tile_m = definitions
                        .paths()
                        .filter(|path| path.elevated)
                        .map(|path| path.width_cm)
                        .max()
                        .map_or(0.0, |width| f32::from(width) * 0.01);
                    if widest_tile_m <= 0.0 {
                        return false;
                    }
                    let offset = Vec3::new(widest_tile_m, 0.0, widest_tile_m) * 0.5;
                    let min = grid.position_cell(position - offset).truncate();
                    let max = grid.position_cell(position + offset).truncate();
                    path_tiles
                        .iter_many(topology.paths_within(min, max))
                        .filter_map(Result::ok)
                        .any(|(path, surface, transform)| {
                            definitions
                                .find_path(path.definition)
                                .is_some_and(|definition| {
                                    definition.elevated
                                        && is_strictly_above_path_tile(
                                            position,
                                            f32::from(definition.width_cm) * 0.01,
                                            surface,
                                            transform,
                                        )
                                })
                        })
                }),
            underwater: view.underwater,
            vehicle: view.vehicle,
        };
        // Include stationary objects as well as moving animals.
        for (entity, member, species, definition, transform, visible, colliders, behavior_task) in
            &subjects
        {
            if member.root != capture.world || !visible.get() {
                continue;
            }
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
            let distance_squared = capture
                .camera_position
                .distance_squared(coverage.world_center);
            if distance_squared > policy.maximum_subject_distance_squared
                || coverage.visible_fraction < policy.minimum_visible_fraction
                || (distance_squared > policy.close_distance_squared
                    && coverage.screen_fraction < policy.minimum_screen_fraction)
            {
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
            if !facts.unoccluded {
                continue;
            }
            let variant = semantic_queries
                .animal_variants
                .get(entity)
                .ok()
                .map(|variant| variant.0);
            let object =
                definitions.and_then(|view| view.find_object(variant.unwrap_or(definition)));
            let play_set = behavior_task
                .filter(|task| task.phase == BehaviorTaskExecutionPhase::Set)
                .and_then(|task| {
                    behavior_document_assets
                        .get(&task.document)?
                        .behavior_set_at_index(task.declaration)
                })
                .map(|set| set.name.clone());
            let semantics = PhotoSemanticEvidence {
                animal: species.is_some(),
                guest: semantic_queries
                    .entity_kinds
                    .get(entity)
                    .is_ok_and(|(_, guest, _, _)| guest.is_some()),
                position_mm: (transform.translation() * 1000.0).round().as_ivec3(),
                animal_variant: variant,
                play_set,
                type_ancestry: object
                    .map_or_else(Vec::new, |object| object.behavior_type_identifiers.clone()),
                rarity_percent: variant
                    .and_then(|variant| {
                        definitions?.find_animal_adoption_species_eligibility(variant)
                    })
                    .map(|eligibility| eligibility.rarity_fame_percent as u16),
                behavior: behavior_task.map(BehaviorTaskExecutionState::program),
                animation: AnimationPresentations::for_subject(
                    entity,
                    semantic_queries.animation_presentations.get(entity).ok(),
                )
                .find_map(|controller| {
                    let (state, controller) = semantic_queries.animations.get(controller).ok()?;
                    animation_set_assets
                        .get(&controller.animation_set_asset)?
                        .animation_graph_nodes
                        .get(state.current_animation_graph_node_index as usize)
                        .map(|node| node.animation_graph_node_asset_key.clone())
                }),
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
                in_water: {
                    let position = transform.translation();
                    semantic_queries
                        .surfaces
                        .water_surface_height(member.root, position.xz())
                        .is_some_and(|surface| position.y <= surface)
                },
                show_trick: semantic_queries
                    .show_tricks
                    .get(entity)
                    .ok()
                    .map(|fact| fact.0),
                fossil: semantic_queries
                    .fossils
                    .get(entity)
                    .ok()
                    .and_then(|(assembly, slots)| {
                        let definitions = semantic_queries
                            .world_definitions
                            .get(&semantic_queries.world_definition_assets)?;
                        let set = definitions.find_fossil_set(assembly.fossil_set_identifier())?;
                        Some(PhotographableFossil {
                            set: assembly.fossil_set_identifier(),
                            complete: slots
                                .iter()
                                .filter(|slot| {
                                    semantic_queries.occupied_fossil_slots.contains(*slot)
                                })
                                .count()
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
                mother: semantic_queries.lineage.parents(entity).mother,
                father: semantic_queries.lineage.parents(entity).father,
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
                name: semantic_queries
                    .names
                    .get(entity)
                    .ok()
                    .map(|name| name.as_str().to_owned()),
                contained: semantic_queries
                    .containment
                    .get(entity)
                    .is_ok_and(|fact| fact.is_contained),
                task: capture_photo_subject_task_evidence(
                    entity,
                    behavior_task,
                    &semantic_queries.continuations,
                    &behavior_document_assets,
                    &semantic_queries.entity_kinds,
                    definitions,
                ),
                contained_entities: capture_photo_subject_contained_entities(
                    entity,
                    &memberships,
                    &semantic_queries.entity_kinds,
                ),
            };
            evidence.0.push(PhotoEvidence {
                entity,
                definition,
                facts,
                semantics,
            });
        }
        evidence
            .0
            .sort_unstable_by_key(|subject| subject.entity.to_bits());
        commands
            .entity(capture_entity)
            .insert(PhotoSubjectsCollected);
    }
}

/// Records docked occupants, excluding queue waiters.
fn capture_photo_subject_contained_entities<'w, 's>(
    container: Entity,
    memberships: &InteractionContainerMemberships,
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
    for entity in memberships.docked_member_entities_in_container(container) {
        let Ok((species, guest, definition, inspectable)) = entity_kinds.get(entity) else {
            return PhotoSubjectContainedEntitiesEvidence::UnsupportedAtCapture;
        };
        let Some(definition) =
            resolve_photo_subject_contained_entity_kind(species, guest, definition, inspectable)
        else {
            return PhotoSubjectContainedEntitiesEvidence::UnsupportedAtCapture;
        };
        contained.push(PhotoSubjectContainedEntityEvidence {
            entity,
            definition,
            guest: guest.is_some(),
            animal: species.is_some(),
        });
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
    actor: Entity,
    behavior_task: Option<&BehaviorTaskExecutionState>,
    continuations: &BehaviorContinuationFrames,
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
    definitions: Option<WorldDefinitionsView<'_>>,
) -> PhotoSubjectTaskEvidence {
    let Some(behavior_task) = behavior_task else {
        return PhotoSubjectTaskEvidence::NoCurrentTask;
    };
    // Resolve the nearest non-Set frame, as reservation_tag does. A missing
    // declaration must not fall through to an older task.
    let Some(selected_task) =
        select_nearest_non_set_behavior_task_frame(actor, behavior_task, continuations)
    else {
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
        definitions,
    )
}

struct SelectedBehaviorTaskFrame<'a> {
    document: &'a Handle<BehaviorDocumentAsset>,
    declaration: usize,
    /// Return frames do not carry a separate target; nested sets inherit the
    /// execution state's target, so selection keeps that same task ownership.
    target: Option<Entity>,
}

fn select_nearest_non_set_behavior_task_frame<'a>(
    actor: Entity,
    behavior_task: &'a BehaviorTaskExecutionState,
    continuations: &'a BehaviorContinuationFrames,
) -> Option<SelectedBehaviorTaskFrame<'a>> {
    std::iter::once((
        &behavior_task.document,
        behavior_task.declaration,
        behavior_task.phase,
    ))
    .chain(
        continuations
            .frames(actor)
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
    definitions: Option<WorldDefinitionsView<'_>>,
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
    let target_object = target_definition.and_then(|id| definitions?.find_object(id));
    let target_type = target_object
        .and_then(|object| object.audio_cue_prefixes.first())
        .cloned()
        .or_else(|| target_definition.map(|id| id.to_lowercase_hexadecimal_string()))
        .unwrap_or_default();
    PhotoSubjectTaskEvidence::CurrentTask {
        authored_task_name,
        target_definition,
        target_type,
        target_ancestry: target_object
            .map_or_else(Vec::new, |object| object.behavior_type_identifiers.clone()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CollisionScreenCoverage {
    screen_fraction: f32,
    visible_fraction: f32,
    world_center: Vec3,
}

/// Projects each collider's bounding corners and clips their union
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
        for x in [bounds.min.x, bounds.max.x] {
            for y in [bounds.min.y, bounds.max.y] {
                for z in [bounds.min.z, bounds.max.z] {
                    let corner = camera.world_to_ndc(camera_transform, Vec3::new(x, y, z))?;
                    if !(0.0..=1.0).contains(&corner.z) {
                        return None;
                    }
                    ndc_min = ndc_min.min(corner.truncate());
                    ndc_max = ndc_max.max(corner.truncate());
                }
            }
        }
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
        screen_fraction: screen_area,
        visible_fraction: (projected_area > 0.0)
            .then(|| clipped_extent.element_product() / projected_area)
            .unwrap_or(0.0),
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
        screen_permille: convert_fraction_to_rounded_permille(coverage.screen_fraction),
        viewport_visible_permille: convert_fraction_to_rounded_permille(coverage.visible_fraction),
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

fn is_strictly_above_path_tile(
    camera: Vec3,
    tile_width: f32,
    surface: &TileSurface,
    transform: &GlobalTransform,
) -> bool {
    let half_width = tile_width * 0.5;
    let local = transform.affine().inverse().transform_point3(camera);
    if !local.is_finite()
        || local.xz().min_element() < -half_width
        || local.xz().max_element() >= half_width
    {
        return false;
    }
    // The whole tile must be below the camera, including its highest corner.
    let corners = [
        (-half_width, -half_width),
        (half_width, -half_width),
        (half_width, half_width),
        (-half_width, half_width),
    ];
    corners
        .into_iter()
        .zip(surface.height_cm)
        .all(|((x, z), height)| {
            camera.y
                > transform
                    .transform_point(Vec3::new(x, f32::from(height) * 0.01, z))
                    .y
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_shot_requires_the_entire_tile_below_the_camera() {
        let transform = GlobalTransform::from_translation(Vec3::Y * 2.0);
        let slopes = TileSurface {
            height_cm: [0, 25, 100, 20, 50],
        };
        assert!(!is_strictly_above_path_tile(
            Vec3::Y * 3.0,
            2.0,
            &slopes,
            &transform
        ));
        assert!(is_strictly_above_path_tile(
            Vec3::Y * 3.01,
            2.0,
            &slopes,
            &transform
        ));
        assert!(!is_strictly_above_path_tile(
            Vec3::new(2.0, 4.0, 0.0),
            2.0,
            &slopes,
            &transform
        ));
    }
}
