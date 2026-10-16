use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animals::{UiAnimalAction, UiAnimalGender};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::world_entity_crating::RemoveWorldEntityFromCrate;

use super::{
    animal_adoption_contracts::{AnimalCatalogueGender, DeclineCurrentAnimalAdoptionOffers},
    animal_birth_and_release_contracts::ReleaseAnimal,
    types::{Animal, ReleaseRestricted},
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiAnimalActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn consume_animal_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    nodes: Query<(&UiAnimalActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected: Res<SelectedEntity>,
    animals: Query<Has<ReleaseRestricted>, With<Animal>>,
    mut uncrate: MessageWriter<RemoveWorldEntityFromCrate>,
    mut releases: MessageWriter<ReleaseAnimal>,
    mut decline_adoption_offers: MessageWriter<DeclineCurrentAnimalAdoptionOffers>,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiAnimalAction::SetAdoptionCatalogueGenderFilter { gender } => {
                    let sex = match gender {
                        UiAnimalGender::Female => openzt2_game_data::species::Sex::Female,
                        UiAnimalGender::Male => openzt2_game_data::species::Sex::Male,
                    };
                    commands.entity(owner.0).insert(AnimalCatalogueGender(sex));
                }
                UiAnimalAction::ReleaseSelectedAnimalFromCrateIntoZoo => {
                    if let Some(animal) = selected.0.filter(|entity| animals.get(*entity).is_ok()) {
                        uncrate.write(RemoveWorldEntityFromCrate(animal));
                    }
                }
                UiAnimalAction::ReleaseSelectedAnimalToWild => {
                    if let Some(animal) = selected
                        .0
                        .filter(|entity| animals.get(*entity).is_ok_and(|restricted| !restricted))
                    {
                        releases.write(ReleaseAnimal(animal));
                    }
                }
                UiAnimalAction::DeclineAllCurrentAdoptionOffers => {
                    decline_adoption_offers.write(DeclineCurrentAnimalAdoptionOffers);
                }
            }
        }
    }
}
