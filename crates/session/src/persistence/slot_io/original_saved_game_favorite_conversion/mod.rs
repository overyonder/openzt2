use std::io;

use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;

pub(super) fn read_original_saved_favourite_animal_trend(
    manager: &OrderedSourceDocumentNode,
    mut resolve_species: impl FnMut(&str) -> io::Result<AssetId>,
) -> io::Result<GuestFavouriteAnimalTrend> {
    if manager.name != "ZTFavoriteAnimalMgr" {
        return Err(invalid("expected ZTFavoriteAnimalMgr"));
    }
    let mut species = [None; 2];
    for (index, name) in ["favoriteAnimal1", "favoriteAnimal2"]
        .into_iter()
        .enumerate()
    {
        let value = required(manager, name)?;
        if !value.is_empty() {
            species[index] = Some(resolve_species(value)?);
        }
    }
    let spawn_count = required(manager, "spawnCount")?
        .parse::<u32>()
        .map_err(|_| invalid("ZTFavoriteAnimalMgr spawnCount is not u32"))?;
    let index = required(manager, "spawnThresholdIndex")?
        .parse::<u8>()
        .map_err(|_| invalid("ZTFavoriteAnimalMgr spawnThresholdIndex is not u8"))?;
    GuestFavouriteAnimalTrend::from_saved_state(species, spawn_count, index)
        .ok_or_else(|| invalid("ZTFavoriteAnimalMgr spawnThresholdIndex must be zero or one"))
}

fn required<'a>(node: &'a OrderedSourceDocumentNode, name: &str) -> io::Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| invalid(format!("{} has no {name}", node.name)))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
