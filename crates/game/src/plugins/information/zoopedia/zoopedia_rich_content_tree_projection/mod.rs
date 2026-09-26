use bevy::prelude::*;

use crate::plugins::information::zoopedia::zoopedia_rich_content_record_projection::spawn_localized_zoopedia_rich_content_record_entity;
use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaRichContentRecordProjectionInput,
    LocalizedZoopediaRichContentTreeProjectionInput, ZoopediaLocalizedRichContentRoot,
};

pub(super) fn replace_authored_text_with_localized_zoopedia_rich_content_tree(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projected_roots: &Query<(Entity, &ZoopediaLocalizedRichContentRoot)>,
    input: LocalizedZoopediaRichContentTreeProjectionInput<'_>,
) {
    retire_localized_zoopedia_rich_content_tree(
        commands,
        input.authored_text_entity,
        projected_roots,
    );
    let Some(localized_content_records) = input
        .localization
        .find_localized_rich_content(input.localized_content_key)
    else {
        return;
    };
    let rich_content_root_entity = commands
        .spawn((
            ZoopediaLocalizedRichContentRoot {
                text: input.authored_text_entity,
            },
            Node {
                width: Val::Percent(100.0),
                height: Val::Auto,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::FlexStart,
                align_items: AlignItems::FlexStart,
                overflow: Overflow::clip(),
                ..default()
            },
            // This tree replaces the authored text node inside the same
            // rectangle. Text-run offsets belong to the retired glyph run.
            UiTransform::IDENTITY,
            ChildOf(input.authored_text_entity),
        ))
        .id();
    let mut projected_record_entities = Vec::with_capacity(localized_content_records.len());
    let mut inline_text_entities = Vec::with_capacity(localized_content_records.len());
    let mut parents_with_projected_children = vec![false; localized_content_records.len() + 1];
    let mut has_link_content: Vec<bool> = localized_content_records
        .iter()
        .map(|record| record.linked_subject_path.is_some())
        .collect();
    let mut has_block_content: Vec<bool> = localized_content_records
        .iter()
        .map(|record| {
            matches!(
                record.rich_content_kind,
                openzt2_game_data::localization::LocalizationRichContentKind::Cell
                    | openzt2_game_data::localization::LocalizationRichContentKind::Image
            )
        })
        .collect();
    for index in (0..localized_content_records.len()).rev() {
        if let Some(parent) = localized_content_records[index].parent_rich_content_node_index {
            let child_has_block_content = has_block_content[index];
            has_block_content[parent as usize] |= child_has_block_content;
            let child_has_link_content = has_link_content[index];
            has_link_content[parent as usize] |= child_has_link_content;
        }
    }
    for (record_index, source_record) in localized_content_records.iter().enumerate() {
        let source_parent_index = source_record
            .parent_rich_content_node_index
            .unwrap_or(u32::MAX);
        let parent_entity = if source_parent_index == u32::MAX {
            rich_content_root_entity
        } else {
            projected_record_entities
                .get(source_parent_index as usize)
                .copied()
                .unwrap_or(rich_content_root_entity)
        };
        let parent_slot = source_record
            .parent_rich_content_node_index
            .map_or(localized_content_records.len(), |parent| parent as usize);
        let has_preceding_sibling =
            std::mem::replace(&mut parents_with_projected_children[parent_slot], true);
        let is_text_span = source_record
            .parent_rich_content_node_index
            .and_then(|parent| inline_text_entities.get(parent as usize))
            .copied()
            .unwrap_or(false);
        let width = source_record
            .rich_content_presentation
            .width_pixels
            .map_or(Val::Auto, Val::Px);
        let height = source_record
            .rich_content_presentation
            .height_pixels
            .map_or(Val::Auto, Val::Px);
        let flex_shrink = if source_record
            .rich_content_presentation
            .width_pixels
            .is_some()
        {
            0.0
        } else {
            1.0
        };
        let projected_record_entity = spawn_localized_zoopedia_rich_content_record_entity(
            commands,
            LocalizedZoopediaRichContentRecordProjectionInput {
                asset_server,
                source_record,
                source_records: localized_content_records,
                zoopedia_page_entity: input.zoopedia_page_entity,
                parent_entity,
                width,
                height,
                flex_shrink,
                base_text_font: input.base_text_font,
                base_physical_font_size_pixels: input.base_physical_font_size_pixels,
                base_text_color: input.base_text_color,
                has_preceding_sibling,
                has_block_content: has_block_content[record_index],
                has_link_content: has_link_content[record_index],
                is_text_span,
            },
        );
        projected_record_entities.push(projected_record_entity);
        inline_text_entities.push(
            is_text_span
                || (!has_block_content[record_index]
                    && matches!(
                        source_record.rich_content_kind,
                        openzt2_game_data::localization::LocalizationRichContentKind::Text
                    )),
        );
    }
}

pub(super) fn retire_localized_zoopedia_rich_content_tree(
    commands: &mut Commands,
    authored_text_entity: Entity,
    projected_roots: &Query<(Entity, &ZoopediaLocalizedRichContentRoot)>,
) {
    projected_roots
        .iter()
        .filter(|(_, projected_root)| projected_root.text == authored_text_entity)
        .for_each(|(projected_root_entity, _)| {
            commands.entity(projected_root_entity).despawn();
        });
}
