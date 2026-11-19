use bevy::prelude::*;
use openzt2_game_assets::{
    graphics_settings_presets::{GraphicsDetailPreset, GraphicsSettingsPresets},
    graphics_settings_types::{GraphicsEffectQuality, GraphicsSettings, ThreeLevelGraphicsDetail},
};
use openzt2_game_data::ui_document::action::UiTrigger;

use openzt2_interface_state::{
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
    let presets = GraphicsSettingsPresets::from_test_source(
        br#"<UserSettings><settings>
        <environmentDetail low="0" medium="1" high="2">
            <setting><ambientsOn value="false"/><skyboxQuality value="1"/></setting>
            <setting><ambientsOn value="true"/><skyboxQuality value="2"/></setting>
            <setting><ambientsOn value="true"/><skyboxQuality value="2"/></setting>
        </environmentDetail>
        <viewDistance low="0" medium="1" high="2"/>
        <waterDetail low="1" medium="2" high="3"/>
        <shadowDetail low="0" medium="2" high="3"/>
        <screenDetail low="0" medium="1" high="1"/>
        <shaderDetail low="0" medium="1" high="2"/>
    </settings></UserSettings>"#,
    )
    .unwrap();
    for (detail, multisample_count) in [
        (GraphicsDetailPreset::Highest, 4),
        (GraphicsDetailPreset::High, 4),
        (GraphicsDetailPreset::Medium, 1),
    ] {
        let preset = presets.settings(detail, multisample_count);
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
    app.add_message::<UiNodeActivated>().add_systems(
        Update,
        copy_changed_graphics_detail_slider_levels_into_settings_draft,
    );
    let initial_graphics = GraphicsSettings {
        view_distance: ThreeLevelGraphicsDetail::Medium,
        ..default()
    };
    let owner = app
        .world_mut()
        .spawn(OptionsScreenDisplayGraphicsAndOnlineMessageDraft {
            graphics: initial_graphics,
            ..default()
        })
        .id();
    let slider = app
        .world_mut()
        .spawn((
            GraphicsDetailSetting::ViewDistance,
            UiValue(2),
            ChildOf(owner),
        ))
        .id();
    app.world_mut().write_message(UiNodeActivated {
        source: ActionSource::System,
        node: slider,
        trigger: UiTrigger::Change,
    });
    app.update();

    let draft = app
        .world()
        .get::<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>(owner)
        .unwrap();
    assert_eq!(
        draft.graphics,
        GraphicsSettings {
            view_distance:
                openzt2_game_assets::graphics_settings_types::ThreeLevelGraphicsDetail::High,
            ..initial_graphics
        }
    );
    assert!(draft.dirty_graphics);
}
