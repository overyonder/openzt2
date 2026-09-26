//! Borrowed subject identities used when entering an authored behavior set.

use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::guests::guest_simulation_types::GuestArchetype;
use crate::plugins::staff::staff_employment_types::StaffRole;

pub(super) fn behavior_subject_type_identifiers<'a>(
    species: Option<&SpeciesHandle>,
    staff: Option<&StaffRole>,
    guest: Option<&GuestArchetype>,
    definitions: WorldDefinitionsView<'a>,
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
}
