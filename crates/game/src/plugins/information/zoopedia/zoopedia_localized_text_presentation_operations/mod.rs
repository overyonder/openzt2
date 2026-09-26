use bevy::prelude::*;

use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaTextBackgroundProjectionInput,
    LocalizedZoopediaTextPresentationProjectionInput, ZoopediaLocalizedTextBackground,
};

pub(super) fn project_localized_text_background_to_authored_ui_node(
    commands: &mut Commands,
    server: &AssetServer,
    backgrounds: &mut Query<
        (
            Entity,
            &ZoopediaLocalizedTextBackground,
            &mut ImageNode,
            &mut Node,
        ),
        Without<UiTextBinding>,
    >,
    input: LocalizedZoopediaTextBackgroundProjectionInput<'_>,
) {
    let existing = backgrounds
        .iter_mut()
        .find(|(_, background, _, _)| background.text == input.authored_text_entity);
    let Some(presentation) = input
        .localization
        .find_localized_text_presentation(input.localized_content_key)
    else {
        if let Some((entity, _, _, _)) = existing {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(image_path) = presentation.background_image_asset_path.as_deref() else {
        if let Some((entity, _, _, _)) = existing {
            commands.entity(entity).despawn();
        }
        return;
    };
    let image = server.load(crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(image_path));
    let left = add_pixel_offset_to_authored_ui_value(
        input.authored_text_node.left,
        presentation.background_offset_pixels[0],
    );
    let top = add_pixel_offset_to_authored_ui_value(
        input.authored_text_node.top,
        presentation.background_offset_pixels[1],
    );
    let width = Val::Px(presentation.background_extent_pixels[0]);
    let height = Val::Px(presentation.background_extent_pixels[1]);
    if let Some((_, _, mut image_node, mut node)) = existing {
        image_node.image = image;
        node.left = left;
        node.top = top;
        node.width = width;
        node.height = height;
        return;
    }
    commands.spawn((
        ZoopediaLocalizedTextBackground {
            text: input.authored_text_entity,
        },
        ImageNode::new(image),
        Node {
            position_type: PositionType::Absolute,
            left,
            top,
            width,
            height,
            ..default()
        },
        ZIndex(-1),
        ChildOf(input.authored_parent_entity),
    ));
}

fn add_pixel_offset_to_authored_ui_value(authored_value: Val, pixel_offset: f32) -> Val {
    match authored_value {
        Val::Px(authored_pixel_value) => Val::Px(authored_pixel_value + pixel_offset),
        _ => Val::Px(pixel_offset),
    }
}

pub(super) fn apply_localized_text_presentation_to_authored_ui_node(
    input: LocalizedZoopediaTextPresentationProjectionInput<'_>,
) {
    let Some(presentation) = input
        .localization
        .find_localized_text_presentation(input.localized_content_key)
    else {
        return;
    };
    if let Some(color_rgba) = presentation.text_color_rgba {
        input.authored_text_color.0 =
            Color::srgba_u8(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]);
    }
    if let Some(font_size) = presentation.font_size_pixels {
        input.authored_text_font.font_size = FontSize::Px(font_size);
    }
    if presentation.bold_text {
        input.authored_text_font.weight = FontWeight::BOLD;
    }
    if input.apply_content_offset {
        if let Some(offset) = presentation.content_offset_pixels {
            input.authored_transform.translation = Val2::px(offset[0], offset[1]);
        }
    }
}
