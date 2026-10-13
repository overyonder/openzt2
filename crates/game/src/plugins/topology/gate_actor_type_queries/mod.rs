use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::AssetId;

use crate::plugins::{
    animal_lifecycle::types::AnimalVariant, guests::guest_simulation_types::GuestArchetype,
    locomotion::locomotion_types::NavAgent, world_spawn::world_membership_types::DefinitionId,
};
use openzt2_game_assets::{
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::{
            WorldDefinitions, WorldDefinitionsView,
        },
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};

/// Borrow the same resolved object ancestry used by authored task selection.
#[derive(SystemParam)]
pub(crate) struct GateActorTypeQueries<'w, 's> {
    assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    active: Option<Res<'w, WorldDefinitions>>,
    species_assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    species_index: Option<Res<'w, SpeciesAssets>>,
    actors: Query<
        'w,
        's,
        (
            Option<&'static DefinitionId>,
            Option<&'static GuestArchetype>,
            Option<&'static AnimalVariant>,
        ),
        With<NavAgent>,
    >,
}

impl GateActorTypeQueries<'_, '_> {
    pub(crate) fn definitions(&self) -> Option<WorldDefinitionsView<'_>> {
        self.active.as_ref()?.get(self.assets.as_ref()?)
    }

    pub(crate) fn actor_types(&self, actor: Entity) -> &[AssetId] {
        let Ok((definition, guest, animal)) = self.actors.get(actor) else {
            return &[];
        };
        if let Some(animal) = animal {
            return self
                .species_index
                .as_ref()
                .zip(self.species_assets.as_ref())
                .and_then(|(index, assets)| index.get(assets))
                .and_then(|species| species.find_variant(animal.0))
                .map_or(&[], |variant| {
                    variant.behavior_subject_type_identifiers.as_slice()
                });
        }
        if let Some(guest) = guest {
            return self
                .definitions()
                .and_then(|definitions| definitions.find_guest(guest.0))
                .map_or(&[], |guest| {
                    guest.behavior_subject_type_identifiers.as_slice()
                });
        }
        definition
            .and_then(|definition| self.definitions()?.find_object(definition.0))
            .map_or(&[], |object| object.behavior_type_identifiers.as_slice())
    }
}
