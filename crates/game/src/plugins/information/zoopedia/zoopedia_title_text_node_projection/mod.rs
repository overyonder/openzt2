use bevy::prelude::*;

use crate::plugins::information::projection::text_replacement_operations::replace_projected_ui_text_if_changed;
use crate::plugins::information::zoopedia::zoopedia_localized_text_presentation_operations::apply_localized_text_presentation_to_authored_ui_node;
use crate::plugins::information::zoopedia::zoopedia_rich_content_types::{
    LocalizedZoopediaTextPresentationProjectionInput, LocalizedZoopediaTitleTextNodeProjectionInput,
};

pub(super) fn project_localized_zoopedia_title_to_authored_text_node(
    commands: &mut Commands,
    input: LocalizedZoopediaTitleTextNodeProjectionInput<'_>,
) {
    let localized_title_text = input.localized_title_text.unwrap_or_default();
    if let Some(existing_text) = input.existing_text {
        replace_projected_ui_text_if_changed(&mut existing_text.0, localized_title_text);
    } else {
        commands
            .entity(input.authored_text_entity)
            .insert(Text::new(localized_title_text));
    }
    if let Some(localized_title_key) = input.localized_title_key {
        apply_localized_text_presentation_to_authored_ui_node(
            LocalizedZoopediaTextPresentationProjectionInput {
                localization: input.localization,
                localized_content_key: localized_title_key,
                authored_text_font: input.authored_text_font,
                authored_text_color: input.authored_text_color,
                authored_transform: input.authored_text_transform,
                apply_content_offset: true,
            },
        );
    }
}
