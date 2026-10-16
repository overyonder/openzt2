//! Borrowed subject identities used when entering an authored behavior set.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets};
use openzt2_game_data::AssetId;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

/// Borrow variant ancestry without depending on navigation or presentation readiness.
#[derive(SystemParam)]
pub(super) struct AnimalBehaviorSubjectTypes<'w, 's> {
    assets: Option<Res<'w, Assets<SpeciesAsset>>>,
    index: Option<Res<'w, SpeciesAssets>>,
    variants: Query<'w, 's, &'static AnimalVariant>,
}

impl AnimalBehaviorSubjectTypes<'_, '_> {
    pub(super) fn borrow_for(&self, actor: Entity) -> &[AssetId] {
        let Ok(variant) = self.variants.get(actor) else {
            return &[];
        };
        self.index
            .as_ref()
            .zip(self.assets.as_ref())
            .and_then(|(index, assets)| index.get(assets))
            .and_then(|species| species.find_variant(variant.0))
            .map_or(&[], |variant| {
                variant.behavior_subject_type_identifiers.as_slice()
            })
    }
}

pub(super) fn behavior_subject_type_identifiers<'a>(
    species: Option<&SpeciesHandle>,
    staff: Option<&StaffRole>,
    guest: Option<&GuestArchetype>,
    definitions: WorldDefinitionsView<'a>,
    animal_types: &'a [AssetId],
) -> impl Iterator<Item = AssetId> + Clone + 'a {
    let guest_types = guest
        .and_then(|guest| definitions.find_guest(guest.0))
        .map_or(&[][..], |definition| {
            definition.behavior_subject_type_identifiers.as_slice()
        });
    [
        species.map(|species| species.species),
        species.map(|_| AssetId::from_key("animal")),
        species.map(|_| AssetId::from_key("b_animal")),
        staff.map(|staff| staff.0),
        staff.map(|_| AssetId::from_key("staff")),
    ]
    .into_iter()
    .flatten()
    .chain(guest_types.iter().copied())
    .chain(animal_types.iter().copied())
}
