//! The places a guest or world object holds attached objects in.

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::{
    world_definitions::world_objects::WorldObjectInteractionSlotDefinition, AssetId,
};

/// A holder's authored containers. An attachment that names no container goes
/// to the holder's unnamed container. Holders without one, such as guests and
/// food stands handing over money and food, hold that item in hand: one item,
/// shown, and gone with its holder.
#[derive(Clone, Copy)]
pub(super) struct AttachmentHolder<'a> {
    slots: &'a [WorldObjectInteractionSlotDefinition],
}

#[derive(Clone, Copy)]
pub(super) struct HolderPlace {
    pub(super) capacity: u8,
    pub(super) hides_contents: bool,
    pub(super) owns_contents: bool,
}

impl<'a> AttachmentHolder<'a> {
    pub(super) fn find(definitions: &WorldDefinitionsView<'a>, id: AssetId) -> Option<Self> {
        definitions
            .find_object(id)
            .map(|object| object.interaction_slots.as_slice())
            .or_else(|| {
                definitions
                    .find_guest(id)
                    .map(|guest| guest.interaction_slots.as_slice())
            })
            .map(|slots| Self { slots })
    }

    /// The index of the hand place, past the authored containers.
    fn hand(self) -> Option<usize> {
        (!self
            .slots
            .iter()
            .any(|slot| !slot.is_queue && slot.reservation_tag == AssetId::default()))
        .then_some(self.slots.len())
    }

    /// The place that receives items attached into `container`.
    pub(super) fn place_for(self, container: AssetId) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| {
                !slot.is_queue && slot.reservation_tag == container && slot.capacity != 0
            })
            .or_else(|| {
                (container == AssetId::default())
                    .then(|| self.hand())
                    .flatten()
            })
    }

    pub(super) fn place(self, index: usize) -> Option<HolderPlace> {
        match self.slots.get(index) {
            Some(slot) => Some(HolderPlace {
                capacity: slot.capacity,
                hides_contents: slot.hides_contents,
                owns_contents: slot.owns_contents,
            }),
            None => (Some(index) == self.hand()).then_some(HolderPlace {
                capacity: 1,
                hides_contents: false,
                owns_contents: true,
            }),
        }
    }

    /// Places whose items an animation's detach key releases.
    pub(super) fn held_places(self) -> impl Iterator<Item = usize> + 'a {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| !slot.is_queue && slot.reservation_tag == AssetId::default())
            .map(|(index, _)| index)
            .chain(self.hand())
    }
}
