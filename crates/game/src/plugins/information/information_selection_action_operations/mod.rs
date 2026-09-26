use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::UiInformationAction;

use crate::{
    plugins::input::input_types::ActionSource,
    plugins::{
        animal_health::types::{Disease, Rampaging},
        camera::{
            camera_control_message_types::SetCameraMode, camera_runtime_state_types::CameraMode,
        },
    },
};

use super::{
    entity_selection_types::{Inspectable, SelectionRequest},
    information_selection_action_types::InformationSelectionActionTargets,
};

pub(super) fn apply_authored_information_selection_action(
    authored_action: &UiInformationAction,
    activated_node: Entity,
    action_source: ActionSource,
    targets: &mut InformationSelectionActionTargets,
) {
    match authored_action {
        UiInformationAction::RenameSelected | UiInformationAction::RenameZoo => {
            let renamed_entity = if matches!(authored_action, UiInformationAction::RenameZoo) {
                targets.zoo_roots.single().ok()
            } else {
                targets.selected_entity.0
            };
            if let (Some(selected_entity), Ok(authored_text)) = (
                renamed_entity,
                targets.activated_text_nodes.get(activated_node),
            ) {
                if let Ok(mut live_entity_name) = targets.live_entity_names.get_mut(selected_entity)
                {
                    live_entity_name.set(authored_text.value().to_string());
                }
            }
        }
        UiInformationAction::SelectNextDiseasedAnimal => {
            select_next_matching_animal_and_follow_with_zoo_camera(
                action_source,
                targets,
                |disease, _| disease.is_some(),
            );
        }
        UiInformationAction::SelectNextRampagingAnimal => {
            select_next_matching_animal_and_follow_with_zoo_camera(
                action_source,
                targets,
                |_, rampaging| rampaging.is_some(),
            );
        }
        UiInformationAction::SelectEntityFromSource => {
            let selected_source_entity =
                std::iter::successors(Some(activated_node), |candidate_entity| {
                    targets
                        .entity_parents
                        .get(*candidate_entity)
                        .ok()
                        .map(ChildOf::parent)
                })
                .find_map(|candidate_entity| {
                    targets
                        .information_entity_sources
                        .get(candidate_entity)
                        .ok()
                });
            // The authored information-panel close button sends
            // ZT_SET_SELECTED_ENTITY without a subject to clear selection.
            targets.selection_requests.write(SelectionRequest {
                entity: selected_source_entity.map(|source| source.0),
                source: action_source,
            });
        }
        _ => unreachable!("non-selection action routed to selection action owner"),
    }
}

fn select_next_matching_animal_and_follow_with_zoo_camera(
    action_source: ActionSource,
    targets: &mut InformationSelectionActionTargets,
    animal_matches: impl Fn(Option<&Disease>, Option<&Rampaging>) -> bool,
) {
    let Some(selected_animal) = select_next_matching_animal_in_entity_order(
        targets.selected_entity.0,
        action_source,
        &targets.inspectable_animal_health,
        animal_matches,
        &mut targets.selection_requests,
    ) else {
        return;
    };
    for camera_definition in &targets.zoo_camera_definitions {
        targets.camera_mode_requests.write(SetCameraMode {
            mode: CameraMode::Follow(selected_animal),
            definition: camera_definition.0,
            transition_seconds: None,
        });
    }
}

fn select_next_matching_animal_in_entity_order(
    current_selection: Option<Entity>,
    action_source: ActionSource,
    inspectable_animals: &Query<(Entity, Option<&Disease>, Option<&Rampaging>), With<Inspectable>>,
    animal_matches: impl Fn(Option<&Disease>, Option<&Rampaging>) -> bool,
    selection_requests: &mut MessageWriter<SelectionRequest>,
) -> Option<Entity> {
    let current_entity_bits = current_selection.map(Entity::to_bits);
    let mut first_matching_entity = None::<Entity>;
    let mut next_matching_entity = None::<Entity>;
    for (animal_entity, disease, rampaging) in inspectable_animals.iter() {
        if !animal_matches(disease, rampaging) {
            continue;
        }
        if first_matching_entity
            .is_none_or(|candidate| animal_entity.to_bits() < candidate.to_bits())
        {
            first_matching_entity = Some(animal_entity);
        }
        if current_entity_bits.is_some_and(|bits| animal_entity.to_bits() > bits)
            && next_matching_entity
                .is_none_or(|candidate| animal_entity.to_bits() < candidate.to_bits())
        {
            next_matching_entity = Some(animal_entity);
        }
    }
    let selected_entity = next_matching_entity.or(first_matching_entity)?;
    selection_requests.write(SelectionRequest {
        entity: Some(selected_entity),
        source: action_source,
    });
    Some(selected_entity)
}
