use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::person_name_types::GeneratedPersonNameRowSelection;

pub(crate) fn choose_generated_person_name_rows_from_authored_pool(
    world_definitions: WorldDefinitionsView<'_>,
    person_name_pool_identifier: AssetId,
    random_selection_bits: u64,
) -> Option<GeneratedPersonNameRowSelection> {
    let person_name_pool = world_definitions.find_person_name_pool(person_name_pool_identifier)?;
    let first_name_row = choose_authored_person_name_row(
        &person_name_pool.first_names,
        random_selection_bits.rotate_left(17),
    )?;
    let last_name_row = choose_authored_person_name_row(
        &person_name_pool.last_names,
        random_selection_bits.rotate_right(11),
    )?;
    Some(GeneratedPersonNameRowSelection {
        person_name_pool: person_name_pool_identifier,
        first_name_row,
        last_name_row,
    })
}

pub fn resolve_generated_person_name_text_parts<'world_definitions>(
    world_definitions: WorldDefinitionsView<'world_definitions>,
    generated_person_name: &GeneratedPersonNameRowSelection,
) -> Option<(
    &'world_definitions str,
    &'world_definitions str,
    &'world_definitions str,
)> {
    let person_name_pool =
        world_definitions.find_person_name_pool(generated_person_name.person_name_pool)?;
    let first_name = person_name_pool
        .first_names
        .get(generated_person_name.first_name_row as usize)?;
    let last_name = person_name_pool
        .last_names
        .get(generated_person_name.last_name_row as usize)?;
    Some((
        first_name.as_str(),
        person_name_pool.delimiter.as_str(),
        last_name.as_str(),
    ))
}

fn choose_authored_person_name_row(
    authored_person_names: &[String],
    random_selection_bits: u64,
) -> Option<u32> {
    let authored_person_name_count = u32::try_from(authored_person_names.len()).ok()?;
    (authored_person_name_count != 0)
        .then(|| random_selection_bits as u32 % authored_person_name_count)
}
