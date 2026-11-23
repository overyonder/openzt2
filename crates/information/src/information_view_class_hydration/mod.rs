use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_data::{
    ui_document::action::information::InformationViewCategory,
    world_definitions::world_objects::WorldObjectInformationViewClass,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::{
    animal_lifecycle::types::Animal, aquatic::aquatic_simulation_types::Tank,
    guests::guest_simulation_types::Guest, shows::show_stage_types::ShowStage,
};

use super::information_view_types::InformationViewClass;

pub(super) fn hydrate_information_view_classes_from_canonical_world_definitions(
    assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    sources_changed: Query<
        Entity,
        Or<(
            Changed<Inspectable>,
            Added<Animal>,
            Added<Guest>,
            Added<ShowStage>,
            Added<Tank>,
        )>,
    >,
    mut removed: (
        RemovedComponents<Inspectable>,
        RemovedComponents<Animal>,
        RemovedComponents<Guest>,
        RemovedComponents<ShowStage>,
        RemovedComponents<Tank>,
    ),
    mut classes: Query<
        (
            Entity,
            Option<&Inspectable>,
            Has<Animal>,
            Has<Guest>,
            Has<ShowStage>,
            Has<Tank>,
            Option<&mut InformationViewClass>,
        ),
        Or<(
            With<Inspectable>,
            With<Animal>,
            With<Guest>,
            With<ShowStage>,
            With<Tank>,
            With<InformationViewClass>,
        )>,
    >,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&assets) else {
        return;
    };
    let refresh_all = active_definitions.is_changed();
    let mut dirty = removed
        .0
        .read()
        .chain(removed.1.read())
        .chain(removed.2.read())
        .chain(removed.3.read())
        .chain(removed.4.read())
        .collect::<EntityHashSet>();
    if refresh_all {
        dirty.extend(classes.iter().map(|(entity, ..)| entity));
    } else {
        dirty.extend(sources_changed.iter());
    }
    let mut subjects = classes.iter_many_mut(dirty);
    while let Some(row) = subjects.fetch_next() {
        let Ok((entity, inspectable, animal, guest, stage, tank, current)) = row else {
            continue;
        };
        let category = if animal && inspectable.is_some() {
            Some(InformationViewCategory::Animals)
        } else if guest && inspectable.is_some() {
            Some(InformationViewCategory::Guests)
        } else if stage {
            Some(InformationViewCategory::Shows)
        } else if tank {
            Some(InformationViewCategory::Tanks)
        } else {
            inspectable
                .and_then(|inspectable| definitions.find_object(inspectable.definition))
                .and_then(|definition| definition.view_class.as_ref())
                .map(|view_class| match view_class {
                    WorldObjectInformationViewClass::Building => InformationViewCategory::Buildings,
                    WorldObjectInformationViewClass::Entrance => InformationViewCategory::Entrances,
                    WorldObjectInformationViewClass::Fence => InformationViewCategory::Fences,
                    WorldObjectInformationViewClass::Curb => InformationViewCategory::Curbs,
                    WorldObjectInformationViewClass::ZooWall => InformationViewCategory::ZooWalls,
                    WorldObjectInformationViewClass::Foliage => InformationViewCategory::Foliage,
                })
        };
        match (current, category.map(InformationViewClass)) {
            (Some(mut current), Some(next)) => {
                current.set_if_neq(next);
            }
            (None, Some(next)) => {
                commands.entity(entity).insert(next);
            }
            (Some(_), None) => {
                commands.entity(entity).try_remove::<InformationViewClass>();
            }
            (None, None) => {}
        }
    }
}
