use bevy::prelude::*;
use openzt2_game_data::species::Sex;

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
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::Habitat;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPrefabHydrated;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewRequest;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    animal_adoption_contracts::{
        AdoptAnimal, AnimalPlacementOperation, AnimalSpeciesAwaitingHabitatPlacement,
        BeginAnimalAdoptionPlacement, BeginAnimalRelocation,
    },
    animal_adoption_pricing::resolve_nonnegative_animal_adoption_cost,
    animal_habitat_placement_validation::{
        validate_animal_habitat_against_authored_aquatic_requirements, AnimalAquaticHabitatQueries,
    },
};

pub(super) fn begin_selected_species_adoption_placement_on_construction_cursor(
    mut commands: Commands,
    mut adoption_placement_requests: MessageReader<BeginAnimalAdoptionPlacement>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    construction_cursors: Query<Entity, With<ConstructionCursor>>,
    mut construction_tool_requests: MessageWriter<SelectConstructionTool>,
) {
    let Ok(construction_cursor) = construction_cursors.single() else {
        return;
    };
    let Some(available_species) = species_index.get(&species_assets) else {
        return;
    };
    for request in adoption_placement_requests.read() {
        // Species rows choose the sex from the gender toggle. A variant row
        // names the exact authored entity, including its sex and life stage.
        let (species, variant, sex, preview_definition) =
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
        info!(target: "openzt2_gameplay_journey", ?request, prefab = ?preview_definition,
            "starting adoption placement");
        commands.entity(construction_cursor).insert((
            AnimalSpeciesAwaitingHabitatPlacement {
                species: species.id,
                variant,
                operation: AnimalPlacementOperation::Adoption {
                    sex,
                    offer_slot_index: request.offer_slot_index,
                },
            },
            ObjectPlacementPreviewRequest(preview_definition),
        ));
        construction_tool_requests.write(SelectConstructionTool(ConstructionTool::Placement));
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
    cursors: Query<Entity, With<ConstructionCursor>>,
    mut tools: MessageWriter<SelectConstructionTool>,
) {
    let Ok(cursor) = cursors.single() else {
        return;
    };
    for request in requests.read() {
        let Ok((species, variant, definition)) = animals.get(request.0) else {
            continue;
        };
        commands.entity(cursor).insert((
            AnimalSpeciesAwaitingHabitatPlacement {
                species: species.species,
                variant: Some(variant.0),
                operation: AnimalPlacementOperation::Relocation(request.0),
            },
            ObjectPlacementPreviewRequest(definition.0),
        ));
        tools.write(SelectConstructionTool(ConstructionTool::Placement));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_selected_species_adoption_placement_preview(
    zoo_cash: Res<ZooCash>,
    unlimited_zoo_cash: Option<Res<UnlimitedZooCash>>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    construction_cursors: Query<(
        &ConstructionCursor,
        &AnimalSpeciesAwaitingHabitatPlacement,
        &ObjectPlacementPreviewRequest,
    )>,
    habitat_index: Res<HabitatIndex>,
    topology_grid: Res<TopologyGrid>,
    habitats: Query<&WorldMember, With<Habitat>>,
    adoption_policy: Query<&AnimalAdoptionAvailabilityPolicy, With<WorldRoot>>,
    aquatic_habitats: AnimalAquaticHabitatQueries,
    hydrated_previews: Query<(), With<ObjectPlacementPreviewPrefabHydrated>>,
    mut previews: Query<(Entity, &mut ConstructionPreview)>,
) {
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
    let Some(species) = species_index.find(animal_placement.species) else {
        return;
    };
    for (preview_entity, mut preview) in &mut previews {
        if preview.definition != requested_prefab.0 {
            continue;
        }
        let habitat = locate_habitat_at_world_position(
            &habitat_index,
            Vec2::new(
                preview.transform.translation.x,
                preview.transform.translation.z,
            ),
            *topology_grid,
        );
        let next_validity = if !cursor.over_terrain {
            PlacementValidity::Invalid(PlacementFailure::OutsideMap)
        } else if hydrated_previews.get(preview_entity).is_err() {
            PlacementValidity::Pending
        } else if matches!(
            animal_placement.operation,
            AnimalPlacementOperation::Adoption { .. }
        ) && adoption_policy.single().is_ok_and(|policy| !policy.enabled)
        {
            PlacementValidity::Invalid(PlacementFailure::Locked)
        } else if habitat.is_none_or(|habitat| {
            habitats.get(habitat).is_err()
                || validate_animal_habitat_against_authored_aquatic_requirements(
                    world_definitions,
                    animal_placement.species,
                    habitat,
                    &aquatic_habitats,
                )
                .is_err()
        }) {
            PlacementValidity::Invalid(PlacementFailure::InvalidHabitat)
        } else if matches!(
            animal_placement.operation,
            AnimalPlacementOperation::Relocation(_)
        ) {
            PlacementValidity::Valid {
                cost: openzt2_zoo_state::economy::money_types::Money(0),
            }
        } else {
            let Some(mut adoption_cost) =
                resolve_nonnegative_animal_adoption_cost(world_definitions, species)
            else {
                continue;
            };
            if let Ok(policy) = adoption_policy.single() {
                adoption_cost.0 = adoption_cost
                    .0
                    .saturating_mul(i64::from(policy.multiplier_permille))
                    / 1000;
            }
            if unlimited_zoo_cash.is_none() && zoo_cash.0 .0 < adoption_cost.0 {
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
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    ui_pointer_capture: Res<UiPointerCapture>,
    construction_tool: Res<ConstructionTool>,
    construction_cursors: Query<(
        Entity,
        &AnimalSpeciesAwaitingHabitatPlacement,
        &ObjectPlacementPreviewRequest,
    )>,
    habitat_index: Res<HabitatIndex>,
    topology_grid: Res<TopologyGrid>,
    previews: Query<&ConstructionPreview>,
    mut adoption_requests: MessageWriter<AdoptAnimal>,
    mut construction_tool_requests: MessageWriter<SelectConstructionTool>,
    mut animals: Query<
        (
            &mut Transform,
            Option<&mut avian3d::prelude::Position>,
            Option<&mut avian3d::prelude::Rotation>,
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
    if *construction_tool != ConstructionTool::Placement
        || !primary_pointer.just_pressed
        || ui_pointer_capture.over_ui
    {
        return;
    }
    let Ok((cursor_entity, animal_placement, requested_prefab)) = construction_cursors.single()
    else {
        return;
    };
    let Some(valid_preview) = previews.iter().find(|preview| {
        preview.definition == requested_prefab.0
            && matches!(preview.validity, PlacementValidity::Valid { .. })
    }) else {
        return;
    };
    let Some(habitat) = locate_habitat_at_world_position(
        &habitat_index,
        Vec2::new(
            valid_preview.transform.translation.x,
            valid_preview.transform.translation.z,
        ),
        *topology_grid,
    ) else {
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
                habitat,
                transform: valid_preview.transform,
            });
        }
        AnimalPlacementOperation::Relocation(animal) => {
            let Ok((mut transform, position, rotation, has_task)) = animals.get_mut(animal) else {
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
                openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(animal, &mut commands);
            }
            commands
                .entity(animal)
                .remove::<(
                    openzt2_zoo_state::locomotion::locomotion_types::Destination,
                    openzt2_zoo_state::locomotion::locomotion_types::Docking,
                )>()
                .insert(openzt2_zoo_state::locomotion::locomotion_types::Velocity(
                    Vec3::ZERO,
                ));
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
