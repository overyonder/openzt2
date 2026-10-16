use bevy::prelude::*;
use openzt2_game_assets::graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings};
use openzt2_game_data::ui_document::action::UiTrigger;

use crate::plugins::{
    input::input_types::ActionSource,
    ui::{
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::UiValue,
    },
};

use super::{
    graphics_detail_setting_types::GraphicsDetailSetting,
    graphics_detail_setting_ui_presentation::{
        copy_changed_graphics_detail_slider_levels_into_settings_draft,
        graphics_detail_setting_level, set_graphics_detail_setting_level,
    },
    options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
};

#[test]
fn every_slider_reproduces_the_source_presets_and_has_a_label_per_level() {
    for preset in [
        GraphicsSettings::from_source_highest_detail_preset(),
        GraphicsSettings::from_source_high_detail_preset(),
        GraphicsSettings::from_source_medium_detail_preset(),
    ] {
        let mut rebuilt = GraphicsSettings {
            ambient_effects: !preset.ambient_effects,
            skybox_quality: 0,
            multisample_count: 8,
            physical_animations: !preset.physical_animations,
            prop_attachments: !preset.prop_attachments,
            texture_replacement: !preset.texture_replacement,
            effects: match preset.effects {
                GraphicsEffectQuality::Low => GraphicsEffectQuality::High,
                GraphicsEffectQuality::High => GraphicsEffectQuality::Low,
            },
            ..preset
        };
        for setting in GraphicsDetailSetting::ALL {
            let level = graphics_detail_setting_level(setting, &preset);
            assert!(usize::from(level) < setting.level_localization_keys().len());
            set_graphics_detail_setting_level(setting, level, &mut rebuilt);
        }
        assert_eq!(rebuilt, preset);
    }
}

#[test]
fn moving_a_slider_changes_only_the_draft() {
    let mut app = App::new();
    app.add_message::<UiNodeActivated>()
        .insert_resource(OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
            graphics: GraphicsSettings::from_source_high_detail_preset(),
            ..default()
        })
        .add_systems(
            Update,
            copy_changed_graphics_detail_slider_levels_into_settings_draft,
        );
    let slider = app
        .world_mut()
        .spawn((GraphicsDetailSetting::ViewDistance, UiValue(2)))
        .id();
    app.world_mut().write_message(UiNodeActivated {
        source: ActionSource::System,
        node: slider,
        trigger: UiTrigger::Change,
    });
    app.update();

    let draft = app
        .world()
        .resource::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>();
    assert_eq!(
        draft.graphics,
        GraphicsSettings {
            view_distance:
                openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail::High,
            ..GraphicsSettings::from_source_high_detail_preset()
        }
    );
    assert!(draft.dirty_graphics);
}
