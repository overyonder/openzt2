use std::collections::HashSet;

use openzt2_game_data::{
    world_definitions::catalogue_and_progression::zoopedia_entry_types::ZoopediaEntry, AssetId,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

pub(in crate::plugins::information) fn find_root_zoopedia_entry(
    world_definitions: WorldDefinitionsView<'_>,
) -> Option<&ZoopediaEntry> {
    world_definitions.find_zoopedia_entry(AssetId::from_key("zoopedia_home"))
}

pub(in crate::plugins::information) fn resolve_zoopedia_entry_subject(
    entry: &ZoopediaEntry,
) -> AssetId {
    let authored_subject = AssetId(entry.subject.0);
    if authored_subject == AssetId::default() {
        AssetId(entry.id.0)
    } else {
        authored_subject
    }
}

pub(super) fn collect_visible_zoopedia_entries<'a>(
    world_definitions: WorldDefinitionsView<'a>,
    root_entry: &'a ZoopediaEntry,
    expanded_subjects: &HashSet<AssetId>,
) -> Vec<(&'a ZoopediaEntry, u16)> {
    fn append_visible_entry<'a>(
        world_definitions: WorldDefinitionsView<'a>,
        entry: &'a ZoopediaEntry,
        depth: u16,
        expanded_subjects: &HashSet<AssetId>,
        visited_subjects: &mut HashSet<AssetId>,
        visible_entries: &mut Vec<(&'a ZoopediaEntry, u16)>,
    ) {
        let subject = resolve_zoopedia_entry_subject(entry);
        if !visited_subjects.insert(subject) {
            return;
        }
        visible_entries.push((entry, depth));
        if expanded_subjects.contains(&subject) {
            entry
                .related
                .iter()
                .filter_map(|identifier| world_definitions.find_zoopedia_entry(*identifier))
                .for_each(|child_entry| {
                    append_visible_entry(
                        world_definitions,
                        child_entry,
                        depth.saturating_add(1),
                        expanded_subjects,
                        visited_subjects,
                        visible_entries,
                    );
                });
        }
    }

    let mut visited_subjects = HashSet::new();
    let mut visible_entries = Vec::new();
    append_visible_entry(
        world_definitions,
        root_entry,
        0,
        expanded_subjects,
        &mut visited_subjects,
        &mut visible_entries,
    );
    visible_entries
}

pub(super) fn zoopedia_entry_has_children(
    world_definitions: WorldDefinitionsView<'_>,
    entry: &ZoopediaEntry,
) -> bool {
    entry
        .related
        .iter()
        .any(|identifier| world_definitions.find_zoopedia_entry(*identifier).is_some())
}
