use bevy::prelude::*;

use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use crate::plugins::information::projection::text_replacement_operations::replace_projected_ui_text_if_changed;
use crate::plugins::information::zoopedia::zoopedia_localized_text_presentation_operations::{
    apply_localized_text_presentation_to_authored_ui_node,
    project_localized_text_background_to_authored_ui_node,
};
use crate::plugins::information::zoopedia::zoopedia_rich_content_tree_projection::{
    replace_authored_text_with_localized_zoopedia_rich_content_tree,
    retire_localized_zoopedia_rich_content_tree,
};
use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaBodyTextNodeProjectionInput, LocalizedZoopediaRichContentTreeProjectionInput,
    LocalizedZoopediaTextBackgroundProjectionInput,
    LocalizedZoopediaTextPresentationProjectionInput, ZoopediaLocalizedRichContentRoot,
    ZoopediaLocalizedTextBackground,
};

pub(super) fn project_localized_zoopedia_body_to_authored_text_node(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projected_backgrounds: &mut Query<
        (
            Entity,
            &ZoopediaLocalizedTextBackground,
            &mut ImageNode,
            &mut Node,
        ),
        Without<UiTextBinding>,
    >,
    projected_rich_content_roots: &Query<(Entity, &ZoopediaLocalizedRichContentRoot)>,
    input: LocalizedZoopediaBodyTextNodeProjectionInput<'_>,
) {
    let localized_rich_content = input
        .localization
        .find_localized_rich_content(input.localized_body_key);
    let has_localized_rich_content =
        localized_rich_content.is_some_and(|content| !content.is_empty());
    if !has_localized_rich_content && input.localized_plain_body_text.is_none() {
        return;
    }
    if has_localized_rich_content {
        project_localized_zoopedia_rich_body_to_authored_text_node(
            commands,
            asset_server,
            projected_backgrounds,
            projected_rich_content_roots,
            input,
        );
    } else {
        project_localized_zoopedia_plain_body_to_authored_text_node(
            commands,
            asset_server,
            projected_backgrounds,
            projected_rich_content_roots,
            input,
        );
    }
}

fn project_localized_zoopedia_rich_body_to_authored_text_node(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projected_backgrounds: &mut Query<
        (
            Entity,
            &ZoopediaLocalizedTextBackground,
            &mut ImageNode,
            &mut Node,
        ),
        Without<UiTextBinding>,
    >,
    projected_rich_content_roots: &Query<(Entity, &ZoopediaLocalizedRichContentRoot)>,
    input: LocalizedZoopediaBodyTextNodeProjectionInput<'_>,
) {
    apply_localized_text_presentation_to_authored_ui_node(
        LocalizedZoopediaTextPresentationProjectionInput {
            localization: input.localization,
            localized_content_key: input.localized_body_key,
            authored_text_font: &mut *input.authored_text_font,
            authored_text_color: &mut *input.authored_text_color,
            authored_transform: &mut *input.authored_text_transform,
            apply_content_offset: false,
        },
    );
    input.authored_node.overflow = Overflow::clip();
    replace_authored_text_with_localized_zoopedia_rich_content_tree(
        commands,
        asset_server,
        projected_rich_content_roots,
        LocalizedZoopediaRichContentTreeProjectionInput {
            localization: input.localization,
            localized_content_key: input.localized_body_key,
            zoopedia_page_entity: input.zoopedia_page_entity,
            authored_text_entity: input.authored_text_entity,
            base_text_font: &*input.authored_text_font,
            base_physical_font_size_pixels: input.base_physical_font_size_pixels,
            base_text_color: &*input.authored_text_color,
        },
    );
    commands.entity(input.authored_text_entity).remove::<Text>();
    retire_localized_zoopedia_text_background(
        commands,
        projected_backgrounds,
        input.authored_text_entity,
    );
}

fn project_localized_zoopedia_plain_body_to_authored_text_node(
    commands: &mut Commands,
    asset_server: &AssetServer,
    projected_backgrounds: &mut Query<
        (
            Entity,
            &ZoopediaLocalizedTextBackground,
            &mut ImageNode,
            &mut Node,
        ),
        Without<UiTextBinding>,
    >,
    projected_rich_content_roots: &Query<(Entity, &ZoopediaLocalizedRichContentRoot)>,
    input: LocalizedZoopediaBodyTextNodeProjectionInput<'_>,
) {
    apply_localized_text_presentation_to_authored_ui_node(
        LocalizedZoopediaTextPresentationProjectionInput {
            localization: input.localization,
            localized_content_key: input.localized_body_key,
            authored_text_font: &mut *input.authored_text_font,
            authored_text_color: &mut *input.authored_text_color,
            authored_transform: &mut *input.authored_text_transform,
            apply_content_offset: true,
        },
    );
    let Some(localized_plain_body_text) = input.localized_plain_body_text else {
        return;
    };
    if let Some(existing_text) = input.existing_text {
        replace_projected_ui_text_if_changed(&mut existing_text.0, localized_plain_body_text);
    } else {
        commands
            .entity(input.authored_text_entity)
            .insert(Text::new(localized_plain_body_text));
    }
    retire_localized_zoopedia_rich_content_tree(
        commands,
        input.authored_text_entity,
        projected_rich_content_roots,
    );
    project_localized_text_background_to_authored_ui_node(
        commands,
        asset_server,
        projected_backgrounds,
        LocalizedZoopediaTextBackgroundProjectionInput {
            localization: input.localization,
            localized_content_key: input.localized_body_key,
            authored_text_entity: input.authored_text_entity,
            authored_parent_entity: input.authored_parent_entity,
            authored_text_node: input.authored_node,
        },
    );
}

fn retire_localized_zoopedia_text_background(
    commands: &mut Commands,
    projected_backgrounds: &mut Query<
        (
            Entity,
            &ZoopediaLocalizedTextBackground,
            &mut ImageNode,
            &mut Node,
        ),
        Without<UiTextBinding>,
    >,
    authored_text_entity: Entity,
) {
    if let Some((background_entity, _, _, _)) = projected_backgrounds
        .iter_mut()
        .find(|(_, background, _, _)| background.text == authored_text_entity)
    {
        commands.entity(background_entity).despawn();
    }
}
