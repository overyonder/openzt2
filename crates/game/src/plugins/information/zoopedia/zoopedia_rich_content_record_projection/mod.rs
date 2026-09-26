use bevy::prelude::*;
use openzt2_game_data::{localization::LocalizationRichContentKind, AssetId};

use crate::plugins::ui::authored_ui_text_layout_presentation::UiPhysicalFontSize;
use crate::plugins::ui::authored_ui_visual_types::UiSourceRect;

use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaRichContentRecordProjectionInput, ZoopediaRichLink,
};

pub(super) fn spawn_localized_zoopedia_rich_content_record_entity(
    commands: &mut Commands,
    input: LocalizedZoopediaRichContentRecordProjectionInput<'_>,
) -> Entity {
    match input.source_record.rich_content_kind {
        LocalizationRichContentKind::Cell => {
            spawn_localized_zoopedia_rich_content_cell(commands, input)
        }
        LocalizationRichContentKind::Text => {
            spawn_localized_zoopedia_rich_content_text(commands, input)
        }
        LocalizationRichContentKind::Image => {
            spawn_localized_zoopedia_rich_content_image(commands, input)
        }
        LocalizationRichContentKind::LineBreak if input.is_text_span => {
            spawn_localized_zoopedia_rich_content_text(commands, input)
        }
        LocalizationRichContentKind::LineBreak => {
            spawn_localized_zoopedia_rich_content_line_break(commands, input)
        }
    }
}

fn spawn_localized_zoopedia_rich_content_cell(
    commands: &mut Commands,
    input: LocalizedZoopediaRichContentRecordProjectionInput<'_>,
) -> Entity {
    let cell_node = Node {
        box_sizing: BoxSizing::ContentBox,
        width: input.width,
        height: input.height,
        flex_shrink: input.flex_shrink,
        padding: UiRect::axes(
            Val::Px(
                input
                    .source_record
                    .rich_content_presentation
                    .padding_pixels
                    .map_or(0.0, |padding| padding[0]),
            ),
            Val::Px(
                input
                    .source_record
                    .rich_content_presentation
                    .padding_pixels
                    .map_or(0.0, |padding| padding[1]),
            ),
        ),
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_content: AlignContent::FlexStart,
        align_items: AlignItems::FlexStart,
        ..default()
    };
    match input
        .source_record
        .image_asset_path
        .as_deref()
        .map(|image_path| input.asset_server.load(crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(image_path)))
    {
        Some(image_handle) => commands
            .spawn((
                cell_node,
                ImageNode {
                    image: image_handle,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(input.parent_entity),
            ))
            .id(),
        None => commands
            .spawn((cell_node, Pickable::IGNORE, ChildOf(input.parent_entity)))
            .id(),
    }
}

fn spawn_localized_zoopedia_rich_content_text(
    commands: &mut Commands,
    input: LocalizedZoopediaRichContentRecordProjectionInput<'_>,
) -> Entity {
    if input.has_block_content {
        return commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_content: AlignContent::FlexStart,
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
                Pickable::IGNORE,
                ChildOf(input.parent_entity),
            ))
            .id();
    }
    let localized_text = if matches!(
        input.source_record.rich_content_kind,
        LocalizationRichContentKind::LineBreak
    ) {
        if input
            .source_record
            .rich_content_presentation
            .starts_paragraph
        {
            "\n\n"
        } else {
            "\n"
        }
    } else {
        input.source_record.localized_text.as_str()
    };
    let mut text_font = input.base_text_font.clone();
    let mut physical_font_size_pixels = input.base_physical_font_size_pixels;
    let mut font_size_pixels = None;
    let mut text_color_rgba = None;
    let mut bold = false;
    let mut italic = false;
    let mut underlined = false;
    let mut subject_link_path = None;
    let mut current_record = Some(input.source_record);
    while let Some(record) = current_record {
        let presentation = &record.rich_content_presentation;
        font_size_pixels = font_size_pixels.or(presentation.font_size_pixels);
        text_color_rgba = text_color_rgba.or(presentation.text_color_rgba);
        bold |= presentation.bold_text;
        italic |= presentation.italic_text;
        underlined |= presentation.underlined_text;
        subject_link_path = subject_link_path.or(record.linked_subject_path.as_deref());
        current_record = record
            .parent_rich_content_node_index
            .and_then(|parent| input.source_records.get(parent as usize));
    }
    if let Some(authored_font_size_pixels) = font_size_pixels {
        physical_font_size_pixels = authored_font_size_pixels;
        text_font.font_size = FontSize::Px(physical_font_size_pixels);
    }
    if bold {
        text_font.weight = FontWeight::BOLD;
    }
    if italic {
        text_font.style = FontStyle::Italic;
    }
    let text_color = text_color_rgba.map_or(*input.base_text_color, |color_rgba| {
        TextColor(Color::srgba_u8(
            color_rgba[0],
            color_rgba[1],
            color_rgba[2],
            color_rgba[3],
        ))
    });
    let mut text_entity = commands.spawn((
        text_font,
        UiPhysicalFontSize::from_physical_pixels(physical_font_size_pixels),
        text_color,
        ChildOf(input.parent_entity),
    ));
    if input.is_text_span {
        text_entity.insert(TextSpan::new(localized_text));
    } else {
        text_entity.insert((
            Text::new(localized_text),
            TextLayout::new(Justify::Left, LineBreak::WordOrCharacter),
            Node {
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                ..default()
            },
        ));
    }
    if underlined {
        text_entity.insert(Underline);
    }
    if !input.is_text_span && (input.has_link_content || subject_link_path.is_some()) {
        text_entity.insert((Button, bevy::ui::RelativeCursorPosition::default()));
    }
    match subject_link_path {
        Some(subject_link_path) => {
            text_entity.insert(ZoopediaRichLink {
                page: input.zoopedia_page_entity,
                subject: AssetId::from_virtual_path(subject_link_path),
            });
        }
        None if input.is_text_span || !input.has_link_content => {
            text_entity.insert(Pickable::IGNORE);
        }
        None => {}
    }
    text_entity.id()
}

fn spawn_localized_zoopedia_rich_content_image(
    commands: &mut Commands,
    input: LocalizedZoopediaRichContentRecordProjectionInput<'_>,
) -> Entity {
    let Some(image_handle) = input
        .source_record
        .image_asset_path
        .as_deref()
        .map(|image_path| {
            input
                .asset_server
                .load(crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(image_path))
        })
    else {
        return input.parent_entity;
    };
    let source_rectangle = input
        .source_record
        .rich_content_presentation
        .source_rectangle_pixels
        .map(|source| {
            Rect::new(
                source[0],
                source[1],
                source[0] + source[2],
                source[1] + source[3],
            )
        });
    let mut image_entity = commands.spawn((
        ImageNode {
            image: image_handle,
            rect: source_rectangle,
            image_mode: NodeImageMode::Stretch,
            ..default()
        },
        Node {
            width: input.width,
            height: input.height,
            flex_shrink: input.flex_shrink,
            ..default()
        },
        Pickable::IGNORE,
        ChildOf(input.parent_entity),
    ));
    if let Some(source) = input
        .source_record
        .rich_content_presentation
        .source_rectangle_pixels
    {
        image_entity.insert(UiSourceRect(source.map(|value| value as i32)));
    }
    image_entity.id()
}

fn spawn_localized_zoopedia_rich_content_line_break(
    commands: &mut Commands,
    input: LocalizedZoopediaRichContentRecordProjectionInput<'_>,
) -> Entity {
    let source_line_height_pixels = if input.has_preceding_sibling {
        input.base_physical_font_size_pixels
    } else {
        0.0
    };
    let paragraph_separation_pixels = if input
        .source_record
        .rich_content_presentation
        .starts_paragraph
    {
        8.0
    } else {
        0.0
    };
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(source_line_height_pixels + paragraph_separation_pixels),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(input.parent_entity),
        ))
        .id()
}
