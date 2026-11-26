use bevy::prelude::Name;
use openzt2_game_assets::localization::loaded_localization_queries::LoadedLocalizationView;
use openzt2_game_data::AssetId;

pub fn select_authored_person_name(
    localization: LoadedLocalizationView<'_>,
    name_pools_in_binder_order: impl IntoIterator<Item = AssetId>,
    random_selection_bits: u64,
) -> Option<Name> {
    let (id, pool) = name_pools_in_binder_order.into_iter().find_map(|id| {
        localization
            .find_person_name_pool(id)
            .map(|pool| (id, pool))
    })?;
    let first_row = choose_authored_person_name_row(
        pool.first_names.len(),
        random_selection_bits.rotate_left(17),
    );
    let last_row = choose_authored_person_name_row(
        pool.last_names.len(),
        random_selection_bits.rotate_right(11),
    );
    localization
        .render_person_name(id, first_row, last_row)
        .filter(|name| !name.is_empty())
        .map(Name::new)
}

fn choose_authored_person_name_row(count: usize, random_selection_bits: u64) -> Option<u32> {
    let count = u32::try_from(count).ok()?;
    (count != 0).then(|| random_selection_bits as u32 % count)
}
