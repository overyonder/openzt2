use bevy::prelude::*;

use super::{
    super::{
        zoopedia::zoopedia_navigation_types::{ZoopediaHistory, ZoopediaPage},
        zoopedia::zoopedia_rich_content_types::ZoopediaRichLink,
    },
    zoopedia_table_of_contents_types::ZoopediaTableOfContentsRow,
};

pub(in crate::plugins::information) fn navigate_to_subject_from_pressed_zoopedia_table_of_contents_row(
    table_of_contents_rows: Query<
        (&ZoopediaTableOfContentsRow, &Interaction),
        Changed<Interaction>,
    >,
    mut zoopedia_pages: Query<(&mut ZoopediaPage, Option<&mut ZoopediaHistory>)>,
) {
    for (table_of_contents_row, interaction) in &table_of_contents_rows {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok((mut page, history)) =
            zoopedia_pages.get_mut(table_of_contents_row.zoopedia_page_entity)
        else {
            continue;
        };
        page.subject = table_of_contents_row.subject;
        page.section = 0;
        if let Some(mut history) = history {
            history.visit_subject(table_of_contents_row.subject);
        }
    }
}

pub(in crate::plugins::information) fn navigate_to_subject_from_pressed_zoopedia_rich_content_link(
    rich_content_links: Query<&ZoopediaRichLink>,
    pressed_text_blocks: Query<
        (
            &Interaction,
            &bevy::ui::RelativeCursorPosition,
            &bevy::text::ComputedTextBlock,
            &bevy::text::TextLayoutInfo,
            &ComputedNode,
            &UiGlobalTransform,
        ),
        Changed<Interaction>,
    >,
    mut zoopedia_pages: Query<(&mut ZoopediaPage, Option<&mut ZoopediaHistory>)>,
) {
    for (interaction, cursor, block, layout, node, transform) in &pressed_text_blocks {
        if *interaction != Interaction::Pressed || !cursor.cursor_over() {
            continue;
        }
        let Some(cursor) = cursor.normalized else {
            continue;
        };
        let mut text_position =
            (cursor + Vec2::splat(0.5)) * node.size() * node.inverse_scale_factor();
        let matrix = transform.affine().matrix2;
        let horizontal_scale = matrix.x_axis.length();
        let vertical_scale = matrix.y_axis.length();
        if vertical_scale <= f32::EPSILON {
            continue;
        }
        // Undo the same horizontal glyph-aspect correction used by authored
        // text extraction before testing Bevy's shaped run rectangles.
        text_position.x *= horizontal_scale / vertical_scale;
        let Some(rich_content_link) = layout.run_geometry.iter().find_map(|run| {
            if !run.bounds.contains(text_position) {
                return None;
            }
            let entity = block.entities().get(run.section_index)?.entity;
            rich_content_links.get(entity).ok()
        }) else {
            continue;
        };
        let Ok((mut page, history)) = zoopedia_pages.get_mut(rich_content_link.page) else {
            continue;
        };
        let previous_subject = page.subject;
        page.subject = rich_content_link.subject;
        page.section = 0;
        if let Some(mut history) = history {
            history.visit_subject(rich_content_link.subject);
        } else {
            debug!(
                ?previous_subject,
                subject = ?rich_content_link.subject,
                "Zoopedia rich link activated before history hydration"
            );
        }
    }
}
