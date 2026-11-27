use bevy::picking::pointer::{PointerButton, PointerInput};
use bevy::prelude::*;
use openzt2_game_data::species::Sex;
use openzt2_zoo_state::placement::placement_surface_site::PlacementSiteQueries;

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::SelectConstructionTool;
use openzt2_zoo_state::economy::zoo_cash_types::UnlimitedZooCash;
use openzt2_zoo_state::economy::zoo_cash_types::ZooCash;
use openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged;
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::{HabitatCellOf, HabitatIndex};
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPrefabHydrated;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRequest;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::TankGeometry,
    shows::show_stage_types::HabitatShowStages,
    terrain::{
        terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
        terrain_world_sampling::terrain_chunk_at,
    },
};

use openzt2_game_data::species::LifeStage;
use openzt2_world_state::simulation_time::deterministic_random_stream::{
    DeterministicRng, RngDomain, ZooSeed,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use super::{
    animal_adoption_contracts::{
        AdoptAnimal, AnimalPlacementOperation, AnimalSpeciesAwaitingHabitatPlacement,
        BeginAnimalAdoptionPlacement, BeginAnimalRelocation,
    },
    animal_adoption_pricing::resolve_nonnegative_animal_adoption_cost,
    animal_spawn_resolution::select_species_spawn_variant,
};

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct AnimalPlacementSite {
    world: Entity,
    habitat: Option<Entity>,
}

#[derive(Component, Default)]
pub(super) struct AnimalPlacementSequence(u64);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct RemovedPlacementInputs<'w, 's> {
    policy: RemovedComponents<'w, 's, AnimalAdoptionAvailabilityPolicy>,
    cash: RemovedComponents<'w, 's, UnlimitedZooCash>,
    prefabs: RemovedComponents<'w, 's, ObjectPlacementPreviewPrefabHydrated>,
    terrain: RemovedComponents<'w, 's, TerrainChunk>,
    edits: RemovedComponents<'w, 's, EditedTerrainSamples>,
    tank: RemovedComponents<'w, 's, TankGeometry>,
    stages: RemovedComponents<'w, 's, HabitatShowStages>,
}

impl RemovedPlacementInputs<'_, '_> {
    fn read(&mut self) -> bevy::ecs::entity::EntityHashSet {
        self.policy
            .read()
            .chain(self.cash.read())
            .chain(self.prefabs.read())
            .chain(self.terrain.read())
            .chain(self.edits.read())
            .chain(self.tank.read())
            .chain(self.stages.read())
            .collect()
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct PlacementTerrainInputs<'w, 's> {
    assets: Res<'w, Assets<TerrainAsset>>,
    indexes: Query<'w, 's, Ref<'static, TerrainIndex>>,
    chunks: Query<
        'w,
        's,
        (
            Ref<'static, TerrainChunk>,
            Option<Ref<'static, EditedTerrainSamples>>,
        ),
    >,
    tanks: Query<
        'w,
        's,
        (
            Option<Ref<'static, TankGeometry>>,
            Option<Ref<'static, HabitatShowStages>>,
        ),
    >,
    removals: RemovedPlacementInputs<'w, 's>,
}

pub(super) type AnimalPlacementCursor<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static mut AnimalSpeciesAwaitingHabitatPlacement>,
        Option<&'static mut ObjectPlacementPreviewRequest>,
    ),
    With<ConstructionCursor>,
>;

pub(super) fn begin_selected_species_adoption_placement_on_construction_cursor(
    mut commands: Commands,
    mut adoption_placement_requests: MessageReader<BeginAnimalAdoptionPlacement>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    world: Single<(&ZooSeed, &mut AnimalPlacementSequence), With<WorldRoot>>,
    mut construction_cursors: AnimalPlacementCursor,
    mut construction_tool_requests: MessageWriter<SelectConstructionTool>,
) {
    let Ok((cursor, placement, preview)) = construction_cursors.single_mut() else {
        return;
    };
    let Some(available_species) = species_index.get(&species_assets) else {
        return;
    };
    let (seed, mut placement_sequence) = world.into_inner();
    let mut latest = None;
    for request in adoption_placement_requests.read() {
        // Species rows choose the sex from the gender toggle. A variant row
        // names the exact authored entity, including its sex and life stage.
        let (species, requested_variant, sex, default_preview) =
            if let Some(species) = available_species.find(request.species) {
                (species, None, request.sex, species.world_definition)
            } else if let Some((species, variant)) =
                available_species.find_variant_with_species(request.species)
            {
                let sex = match variant.sex {
                    Sex::Any => request.sex,
                    authored_sex => Some(authored_sex),
                };
                (species, Some(variant.id), sex, request.species)
            } else {
                warn!(target: "openzt2_gameplay_journey", ?request,
                    "adoption placement requested an unavailable species");
                continue;
            };
        placement_sequence.0 = placement_sequence.0.wrapping_add(1);
        let mut random = DeterministicRng::from_entity(
            *seed,
            PersistentId(placement_sequence.0),
            RngDomain::Reproduction,
        );
        let Some((variant, sex)) = select_species_spawn_variant(
            available_species,
            species,
            requested_variant,
            sex,
            LifeStage::Adult,
            &mut random,
        ) else {
            continue;
        };
        let preview_definition = definitions
            .get(&definition_assets)
            .filter(|catalogue| catalogue.find_object(variant.id).is_some())
            .map_or(default_preview, |_| variant.id);
        info!(target: "openzt2_gameplay_journey", ?request, prefab = ?preview_definition,
            "starting adoption placement");
        latest = Some((
            AnimalSpeciesAwaitingHabitatPlacement {
                species: species.id,
                variant: Some(variant.id),
                operation: AnimalPlacementOperation::Adoption {
                    sex: Some(sex),
                    offer_slot_index: request.offer_slot_index,
                },
            },
            ObjectPlacementPreviewRequest(preview_definition),
        ));
        construction_tool_requests.write(SelectConstructionTool(ConstructionTool::Placement));
    }
    if let Some((request, preview_request)) = latest {
        set_animal_cursor_placement(
            cursor,
            placement,
            preview,
            request,
            preview_request,
            &mut commands,
        );
    }
}

pub(super) fn begin_animal_relocation_on_construction_cursor(
    mut commands: Commands,
    mut requests: MessageReader<BeginAnimalRelocation>,
    animals: Query<
        (
            &super::types::SpeciesHandle,
            &super::types::AnimalVariant,
            &openzt2_world_state::world_spawn::world_membership_types::DefinitionId,
        ),
        (
            With<super::types::Adoptable>,
            Without<super::types::CratingRestricted>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_zoo_state::animal_health::types::Rampaging>,
        ),
    >,
    mut cursors: AnimalPlacementCursor,
    mut tools: MessageWriter<SelectConstructionTool>,
) {
    let Ok((cursor, placement, preview)) = cursors.single_mut() else {
        return;
    };
    let mut latest = None;
    for request in requests.read() {
        let Ok((species, variant, definition)) = animals.get(request.0) else {
            continue;
        };
        latest = Some((
            AnimalSpeciesAwaitingHabitatPlacement {
                species: species.species,
                variant: Some(variant.0),
                operation: AnimalPlacementOperation::Relocation(request.0),
            },
            ObjectPlacementPreviewRequest(definition.0),
        ));
        tools.write(SelectConstructionTool(ConstructionTool::Placement));
    }
    if let Some((request, preview_request)) = latest {
        set_animal_cursor_placement(
            cursor,
            placement,
            preview,
            request,
            preview_request,
            &mut commands,
        );
    }
}

fn set_animal_cursor_placement(
    cursor: Entity,
    placement: Option<Mut<AnimalSpeciesAwaitingHabitatPlacement>>,
    preview: Option<Mut<ObjectPlacementPreviewRequest>>,
    request: AnimalSpeciesAwaitingHabitatPlacement,
    preview_request: ObjectPlacementPreviewRequest,
    commands: &mut Commands,
) {
    if let Some(mut placement) = placement {
        placement.set_if_neq(request);
    } else {
        commands.entity(cursor).insert(request);
    }
    if let Some(mut preview) = preview {
        preview.set_if_neq(preview_request);
    } else {
        commands.entity(cursor).insert(preview_request);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_selected_species_adoption_placement_preview(
    mut commands: Commands,
    zoo_cash: Single<Ref<ZooCash>>,
    unlimited_zoo_cash: Option<Single<Ref<UnlimitedZooCash>>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    construction_cursors: Query<(
        Ref<ConstructionCursor>,
        Ref<AnimalSpeciesAwaitingHabitatPlacement>,
        Ref<ObjectPlacementPreviewRequest>,
    )>,
    habitat_index: Single<Ref<HabitatIndex>>,
    (cell_owners, mut habitat_changes): (
        Query<&HabitatCellOf>,
        Option<MessageReader<HabitatCellOwnershipChanged>>,
    ),
    topology_grid: Single<Ref<TopologyGrid>>,
    adoption_policy: Query<
        (Entity, Option<Ref<AnimalAdoptionAvailabilityPolicy>>),
        With<WorldRoot>,
    >,
    placement_sites: PlacementSiteQueries,
    hydrated_previews: Query<Ref<ObjectPlacementPreviewPrefabHydrated>>,
    mut terrain: PlacementTerrainInputs,
    mut previews: Query<(
        Entity,
        &mut ConstructionPreview,
        Option<&mut AnimalPlacementSite>,
    )>,
) {
    let habitat_index = habitat_index.into_inner();
    let topology_grid = topology_grid.into_inner();
    let species_index_changed = species_index.is_changed();
    let (
        Ok((cursor, animal_placement, requested_prefab)),
        Some(species_index),
        Some(world_definitions),
    ) = (
        construction_cursors.single(),
        species_index.get(&species_assets),
        active_world_definitions.get(&world_definition_assets),
    )
    else {
        return;
    };
    let Ok((world, availability)) = adoption_policy.single() else {
        return;
    };
    let ownership_changed = habitat_changes.as_mut().is_some_and(|changes| {
        let changed = changes.read().any(|change| change.root == world);
        changes.clear();
        changed
    });
    let removed = terrain.removals.read();
    let inputs_changed = cursor.is_changed()
        || animal_placement.is_changed()
        || requested_prefab.is_changed()
        || zoo_cash.is_changed()
        || unlimited_zoo_cash
            .as_ref()
            .is_some_and(|cash| cash.is_changed())
        || availability.as_ref().is_some_and(DetectChanges::is_changed)
        || removed.contains(&world)
        || habitat_index.is_changed()
        || ownership_changed
        || topology_grid.is_changed()
        || species_assets.is_changed()
        || species_index_changed
        || active_world_definitions.is_changed()
        || world_definition_assets.is_changed();
    let availability = availability.as_deref().copied().unwrap_or_default();
    let Some(species) = species_index.find(animal_placement.species) else {
        return;
    };
    for (preview_entity, mut preview, mut cached_site) in &mut previews {
        if preview.definition != requested_prefab.0 {
            continue;
        }
        let hydrated = hydrated_previews.get(preview_entity).ok();
        let translation = preview.transform.translation;
        let site_changed = inputs_changed || preview.is_changed() || cached_site.is_none();
        let site = if site_changed {
            AnimalPlacementSite {
                world,
                habitat: locate_habitat_at_world_position(
                    &habitat_index,
                    &cell_owners,
                    translation.xz(),
                    *topology_grid,
                ),
            }
        } else if let Some(site) = cached_site.as_deref() {
            *site
        } else {
            continue;
        };
        let terrain_changed = terrain.assets.is_changed()
            || terrain.indexes.get(world).is_ok_and(|index| {
                index.is_changed()
                    || terrain_chunk_at(&index, translation.xz()).is_some_and(|chunk| {
                        removed.contains(&chunk)
                            || terrain.chunks.get(chunk).is_ok_and(|(chunk, edits)| {
                                chunk.is_changed()
                                    || edits.as_ref().is_some_and(DetectChanges::is_changed)
                            })
                    })
            });
        let tank_changed = site.habitat.is_some_and(|habitat| {
            removed.contains(&habitat)
                || terrain.tanks.get(habitat).is_ok_and(|(tank, stages)| {
                    tank.as_ref().is_some_and(DetectChanges::is_changed)
                        || stages.as_ref().is_some_and(DetectChanges::is_changed)
                })
        });
        if !site_changed
            && !terrain_changed
            && !tank_changed
            && !removed.contains(&preview_entity)
            && !hydrated.as_ref().is_some_and(DetectChanges::is_changed)
        {
            continue;
        }
        if let Some(cached_site) = cached_site.as_mut() {
            cached_site.set_if_neq(site);
        } else {
            commands.entity(preview_entity).insert(site);
        }
        let next_validity = if !cursor.over_terrain {
            PlacementValidity::Invalid(PlacementFailure::OutsideMap)
        } else if hydrated.is_none() {
            PlacementValidity::Pending
        } else if matches!(
            animal_placement.operation,
            AnimalPlacementOperation::Adoption { .. }
        ) && !availability.enabled
        {
            PlacementValidity::Invalid(PlacementFailure::Locked)
        } else if let Err(failure) = placement_sites.validate(
            animal_placement
                .variant
                .and_then(|variant| species_index.find_variant(variant))
                .map_or(species.placement_surface_policy, |variant| {
                    variant.placement_surface_policy
                }),
            world,
            site.habitat,
            preview.transform.translation,
        ) {
            PlacementValidity::Invalid(failure)
        } else if matches!(
            animal_placement.operation,
            AnimalPlacementOperation::Relocation(_)
        ) {
            PlacementValidity::Valid {
                cost: openzt2_zoo_state::economy::money_types::Money::ZERO,
            }
        } else {
            let Some(adoption_cost) = resolve_nonnegative_animal_adoption_cost(
                world_definitions,
                species,
                availability.multiplier_permille,
            ) else {
                continue;
            };
            if unlimited_zoo_cash.as_ref().is_none_or(|policy| !policy.0)
                && zoo_cash.0 < adoption_cost
            {
                PlacementValidity::Invalid(PlacementFailure::Unaffordable)
            } else {
                PlacementValidity::Valid {
                    cost: adoption_cost,
                }
            }
        };
        if preview.validity != next_validity {
            preview.validity = next_validity;
        }
    }
}

pub(super) fn confirm_selected_species_adoption_into_habitat(
    mut commands: Commands,
    mut pointer_inputs: MessageReader<PointerInput>,
    ui_pointer_capture: Res<UiPointerCapture>,
    construction_tool: Res<ConstructionTool>,
    construction_cursors: Query<(
        Entity,
        &AnimalSpeciesAwaitingHabitatPlacement,
        &ObjectPlacementPreviewRequest,
    )>,
    previews: Query<(&ConstructionPreview, &AnimalPlacementSite)>,
    mut adoption_requests: MessageWriter<AdoptAnimal>,
    mut construction_tool_requests: MessageWriter<SelectConstructionTool>,
    mut animals: Query<
        (
            &mut Transform,
            Option<&mut avian3d::prelude::Position>,
            Option<&mut avian3d::prelude::Rotation>,
            Option<&mut Velocity>,
            Has<openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState>,
        ),
        (
            With<super::types::Animal>,
            Without<super::types::CratingRestricted>,
            Without<openzt2_zoo_state::animal_health::types::Dead>,
            Without<openzt2_zoo_state::animal_health::types::Rampaging>,
        ),
    >,
) {
    let primary_just_pressed = pointer_inputs.read().any(|input| {
        input.pointer_id.is_mouse() && input.button_just_pressed(PointerButton::Primary)
    });
    pointer_inputs.clear();
    if *construction_tool != ConstructionTool::Placement
        || !primary_just_pressed
        || ui_pointer_capture.over_ui
    {
        return;
    }
    let Ok((cursor_entity, animal_placement, requested_prefab)) = construction_cursors.single()
    else {
        return;
    };
    let Some((valid_preview, site)) = previews.iter().find(|(preview, _)| {
        preview.definition == requested_prefab.0
            && matches!(preview.validity, PlacementValidity::Valid { .. })
    }) else {
        return;
    };
    match animal_placement.operation {
        AnimalPlacementOperation::Adoption {
            sex,
            offer_slot_index,
        } => {
            adoption_requests.write(AdoptAnimal {
                species: animal_placement.species,
                variant: animal_placement.variant,
                sex,
                offer_slot_index,
                world: site.world,
                habitat: site.habitat,
                transform: valid_preview.transform,
            });
        }
        AnimalPlacementOperation::Relocation(animal) => {
            let Ok((mut transform, position, rotation, velocity, has_task)) =
                animals.get_mut(animal)
            else {
                return;
            };
            transform.translation = valid_preview.transform.translation;
            transform.rotation = valid_preview.transform.rotation;
            if let Some(mut position) = position {
                position.0 = transform.translation;
            }
            if let Some(mut rotation) = rotation {
                rotation.0 = transform.rotation;
            }
            if has_task {
                openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(animal, &mut commands);
            }
            commands.entity(animal).remove::<(
                openzt2_zoo_state::locomotion::locomotion_types::Destination,
                openzt2_zoo_state::locomotion::locomotion_types::Docking,
            )>();
            if let Some(mut velocity) = velocity {
                velocity.set_if_neq(Velocity(Vec3::ZERO));
            } else {
                commands.entity(animal).insert(Velocity(Vec3::ZERO));
            }
        }
    }
    commands.entity(cursor_entity).remove::<(
        AnimalSpeciesAwaitingHabitatPlacement,
        ObjectPlacementPreviewRequest,
    )>();
    construction_tool_requests.write(SelectConstructionTool(ConstructionTool::Inspect));
}

pub(super) fn clear_animal_adoption_placement_after_construction_tool_change(
    mut commands: Commands,
    construction_tool: Res<ConstructionTool>,
    construction_cursors: Query<Entity, With<AnimalSpeciesAwaitingHabitatPlacement>>,
) {
    if !construction_tool.is_changed() || *construction_tool == ConstructionTool::Placement {
        return;
    }
    for construction_cursor in &construction_cursors {
        commands.entity(construction_cursor).remove::<(
            AnimalSpeciesAwaitingHabitatPlacement,
            ObjectPlacementPreviewRequest,
        )>();
    }
}
