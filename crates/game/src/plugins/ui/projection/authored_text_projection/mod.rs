use bevy::prelude::*;
use openzt2_game_data::{ui_document::widget_control::UiTextRecord, AssetId};

use crate::plugins::ui::authored_ui_text_layout_presentation::UiSingleLineTextAlignment;
use crate::plugins::ui::localized_ui_text_writing::localized_ui_text;

pub(super) fn apply_authored_text_record_to_bevy_text_and_layout(
    commands: &mut Commands,
    entity: Entity,
    text: &UiTextRecord,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    wraps_text: bool,
) {
    let minimum_height = text.minimum_height;
    let auto_size = text.auto_size;
    if !wraps_text {
        commands.entity(entity).insert(UiSingleLineTextAlignment);
    }
    let literal = (!text.value.is_empty()).then_some(text.value.as_str());
    let localization_key = AssetId(text.localization_key.0);
    if let Some(value) = literal
        .filter(|value| !value.is_empty())
        .or_else(|| localized_ui_text(localization, localization_key))
    {
        commands.entity(entity).insert(Text::new(value));
    }
    commands
        .entity(entity)
        .entry::<Node>()
        .and_modify(move |mut value| {
            // `autosize` replaces the authored region's block extent. Only
            // the explicit text `minimumheight` attribute constrains the
            // measured result; treating UIRegion.h as another minimum makes
            // compact tree rows retain their 30-pixel wrapping box.
            value.min_height = px(minimum_height.max(0) as f32);
            if auto_size {
                // Multi-line text grows vertically inside its authored wrap
                // width. Give Taffy a definite inline constraint so Parley can
                // measure the wrapped block before its auto block size is
                // resolved. Clearing that width turns every paragraph into
                // one unbounded line; single-line labels may still size both
                // axes to their measured content.
                if wraps_text {
                    if let Val::Px(width) = value.width {
                        value.min_width = px(width);
                        value.max_width = px(width);
                    }
                } else {
                    value.width = Val::Auto;
                }
                value.height = Val::Auto;
            }
        });
}
