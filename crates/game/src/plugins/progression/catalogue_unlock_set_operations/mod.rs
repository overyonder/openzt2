use super::unlock_types::UnlockedCatalogueDefinitionSet;

pub(crate) fn catalogue_definition_is_unlocked_at_index(
    unlocked_definitions: &UnlockedCatalogueDefinitionSet,
    definition_index: u32,
) -> bool {
    if definition_index >= unlocked_definitions.definition_count {
        return false;
    }
    let word_index = definition_index as usize / u64::BITS as usize;
    let bit_index = definition_index % u64::BITS;
    unlocked_definitions
        .words
        .get(word_index)
        .is_some_and(|word| word & (1_u64 << bit_index) != 0)
}

pub(crate) fn mark_catalogue_definition_unlocked_at_index(
    unlocked_definitions: &mut UnlockedCatalogueDefinitionSet,
    definition_index: u32,
) -> Option<bool> {
    if definition_index >= unlocked_definitions.definition_count {
        return None;
    }
    let word = unlocked_definitions
        .words
        .get_mut(definition_index as usize / u64::BITS as usize)?;
    let mask = 1_u64 << (definition_index % u64::BITS);
    let changed = *word & mask == 0;
    *word |= mask;
    Some(changed)
}
