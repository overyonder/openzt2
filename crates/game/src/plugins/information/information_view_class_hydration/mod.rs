use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::action::information::InformationViewCategory,
    world_definitions::world_objects::WorldObjectInformationViewClass,
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::aquatic::aquatic_simulation_types::Tank;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::shows::show_stage_types::ShowStage;

use super::{
    entity_selection_types::Inspectable,
    information_view_types::{InformationViewClass, InformationViewClassResolved},
};

pub(super) fn hydrate_information_view_classes_from_canonical_world_definitions(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    inspectable_entities: Query<
        (Entity, &Inspectable, Option<&Guest>, Has<Animal>),
        Without<InformationViewClassResolved>,
    >,
    show_stages: Query<Entity, (With<ShowStage>, Without<InformationViewClass>)>,
    tanks: Query<Entity, (With<Tank>, Without<InformationViewClass>)>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };

    for (entity, inspectable, guest, is_animal) in &inspectable_entities {
        let information_view_category = is_animal
            .then_some(InformationViewCategory::Animals)
            .or_else(|| guest.is_some().then_some(InformationViewCategory::Guests))
            .or_else(|| {
                world_definitions
                    .find_object(inspectable.definition)
                    .and_then(|definition| definition.view_class.as_ref())
                    .map(|view_class| match view_class {
                        WorldObjectInformationViewClass::Building => {
                            InformationViewCategory::Buildings
                        }
                        WorldObjectInformationViewClass::Entrance => {
                            InformationViewCategory::Entrances
                        }
                        WorldObjectInformationViewClass::Fence => InformationViewCategory::Fences,
                        WorldObjectInformationViewClass::Curb => InformationViewCategory::Curbs,
                        WorldObjectInformationViewClass::ZooWall => {
                            InformationViewCategory::ZooWalls
                        }
                        WorldObjectInformationViewClass::Foliage => {
                            InformationViewCategory::Foliage
                        }
                    })
            });

        if let Some(information_view_category) = information_view_category {
            commands.entity(entity).insert((
                InformationViewClass(information_view_category),
                InformationViewClassResolved,
            ));
        } else {
            commands.entity(entity).insert(InformationViewClassResolved);
        }
    }
    for show_stage_entity in &show_stages {
        commands
            .entity(show_stage_entity)
            .insert(InformationViewClass(InformationViewCategory::Shows));
    }
    for tank_entity in &tanks {
        commands
            .entity(tank_entity)
            .insert(InformationViewClass(InformationViewCategory::Tanks));
    }
}
