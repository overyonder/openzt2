use bevy::prelude::*;
use openzt2_game_assets::{
    graphics_settings_types::{
        EnvironmentalDetailObjectDensity, FourLevelGraphicsDetail, GraphicsEffectQuality,
        GraphicsSettings, ShadowDetail, ThreeLevelGraphicsDetail,
    },
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
};
use openzt2_game_data::{
    ui_document::{
        action::UiTrigger, node_property_binding::UiTextPropertyBindingSource,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{SetUiListRowCount, UiListPolicy, UiListRow},
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_integer_range_components::UiMaximum,
    authored_ui_node_projection_components::{UiNodeId, UiValue},
};

use super::{
    graphics_detail_setting_types::GraphicsDetailSetting,
    options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft,
};

const fn three_level(level: ThreeLevelGraphicsDetail) -> u8 {
    match level {
        ThreeLevelGraphicsDetail::Low => 0,
        ThreeLevelGraphicsDetail::Medium => 1,
        ThreeLevelGraphicsDetail::High => 2,
    }
}

const fn three_level_from(level: u8) -> ThreeLevelGraphicsDetail {
    match level {
        0 => ThreeLevelGraphicsDetail::Low,
        1 => ThreeLevelGraphicsDetail::Medium,
        _ => ThreeLevelGraphicsDetail::High,
    }
}

pub(super) const fn graphics_detail_setting_level(
    setting: GraphicsDetailSetting,
    graphics: &GraphicsSettings,
) -> u8 {
    match setting {
        GraphicsDetailSetting::EnvironmentDetail => three_level(graphics.environment_detail),
        GraphicsDetailSetting::LightingDetail => three_level(graphics.lighting_detail),
        GraphicsDetailSetting::ShadowDetail => match graphics.shadow_detail {
            ShadowDetail::Off => 0,
            ShadowDetail::Medium => 1,
            ShadowDetail::High => 2,
            ShadowDetail::Ultra => 3,
        },
        GraphicsDetailSetting::SpecularEffects => graphics.specular_effects as u8,
        GraphicsDetailSetting::TerrainResolution => three_level(graphics.terrain_resolution),
        GraphicsDetailSetting::TerrainTextureSize => three_level(graphics.terrain_texture_size),
        GraphicsDetailSetting::WaterDetail => match graphics.water_detail {
            FourLevelGraphicsDetail::Low => 0,
            FourLevelGraphicsDetail::Medium => 1,
            FourLevelGraphicsDetail::High => 2,
            FourLevelGraphicsDetail::Ultra => 3,
        },
        GraphicsDetailSetting::DetailObjects => match graphics.detail_objects {
            EnvironmentalDetailObjectDensity::Off => 0,
            EnvironmentalDetailObjectDensity::Medium => 1,
            EnvironmentalDetailObjectDensity::High => 2,
        },
        GraphicsDetailSetting::ObjectDetail => three_level(graphics.object_detail),
        GraphicsDetailSetting::ObjectTextureDetail => three_level(graphics.object_texture_detail),
        GraphicsDetailSetting::AnimationDetail => three_level(graphics.animation_detail),
        GraphicsDetailSetting::ViewDistance => three_level(graphics.view_distance),
        GraphicsDetailSetting::MaxOverheadZoom => three_level(graphics.max_overhead_zoom),
        GraphicsDetailSetting::ScreenDetail => graphics.screen_detail as u8,
        GraphicsDetailSetting::UiTextShadows => graphics.ui_text_shadows as u8,
        GraphicsDetailSetting::UiTextTextures => graphics.ui_text_textures as u8,
        GraphicsDetailSetting::ShaderDetail => three_level(graphics.shader_detail),
    }
}

/// Writes one slider position, including the dependent values the source
/// `<setting>` element assigns with it.
pub(super) const fn set_graphics_detail_setting_level(
    setting: GraphicsDetailSetting,
    level: u8,
    graphics: &mut GraphicsSettings,
) {
    match setting {
        GraphicsDetailSetting::EnvironmentDetail => {
            graphics.environment_detail = three_level_from(level);
            graphics.ambient_effects = level > 0;
            graphics.skybox_quality = if level > 0 { 2 } else { 1 };
        }
        GraphicsDetailSetting::LightingDetail => graphics.lighting_detail = three_level_from(level),
        GraphicsDetailSetting::ShadowDetail => {
            graphics.shadow_detail = match level {
                0 => ShadowDetail::Off,
                1 => ShadowDetail::Medium,
                2 => ShadowDetail::High,
                _ => ShadowDetail::Ultra,
            };
        }
        GraphicsDetailSetting::SpecularEffects => graphics.specular_effects = level > 0,
        GraphicsDetailSetting::TerrainResolution => {
            graphics.terrain_resolution = three_level_from(level);
        }
        GraphicsDetailSetting::TerrainTextureSize => {
            graphics.terrain_texture_size = three_level_from(level);
        }
        GraphicsDetailSetting::WaterDetail => {
            graphics.water_detail = match level {
                0 => FourLevelGraphicsDetail::Low,
                1 => FourLevelGraphicsDetail::Medium,
                2 => FourLevelGraphicsDetail::High,
                _ => FourLevelGraphicsDetail::Ultra,
            };
        }
        GraphicsDetailSetting::DetailObjects => {
            graphics.detail_objects = match level {
                0 => EnvironmentalDetailObjectDensity::Off,
                1 => EnvironmentalDetailObjectDensity::Medium,
                _ => EnvironmentalDetailObjectDensity::High,
            };
        }
        GraphicsDetailSetting::ObjectDetail => {
            graphics.object_detail = three_level_from(level);
            graphics.physical_animations = level > 0;
            graphics.prop_attachments = level > 0;
        }
        GraphicsDetailSetting::ObjectTextureDetail => {
            graphics.object_texture_detail = three_level_from(level);
            graphics.texture_replacement = level > 0;
        }
        GraphicsDetailSetting::AnimationDetail => {
            graphics.animation_detail = three_level_from(level);
        }
        GraphicsDetailSetting::ViewDistance => graphics.view_distance = three_level_from(level),
        GraphicsDetailSetting::MaxOverheadZoom => {
            graphics.max_overhead_zoom = three_level_from(level);
        }
        // Localized as "Anti-Aliasing" in the source options screen.
        GraphicsDetailSetting::ScreenDetail => {
            graphics.screen_detail = level > 0;
            graphics.multisample_count = if level > 0 { 4 } else { 1 };
        }
        GraphicsDetailSetting::UiTextShadows => graphics.ui_text_shadows = level > 0,
        GraphicsDetailSetting::UiTextTextures => graphics.ui_text_textures = level > 0,
        GraphicsDetailSetting::ShaderDetail => {
            graphics.shader_detail = three_level_from(level);
            // Every source preset pairs low shaders with low effect quality.
            graphics.effects = if level > 0 {
                GraphicsEffectQuality::High
            } else {
                GraphicsEffectQuality::Low
            };
        }
    }
}

pub(super) fn set_authored_graphics_detail_setting_list_row_count(
    lists: Query<(Entity, Ref<UiListPolicy>)>,
    mut row_counts: MessageWriter<SetUiListRowCount>,
) {
    for (list, policy) in &lists {
        if policy.source == UiWidgetLiveCollectionSource::GraphicsDetailSettings
            && policy.is_added()
        {
            row_counts.write(SetUiListRowCount {
                list,
                count: u16::try_from(GraphicsDetailSetting::ALL.len()).unwrap_or(u16::MAX),
            });
        }
    }
}

/// Fills each authored `optionsslider.xml` row with its setting label, slider
/// range and the draft's current level. The slider keeps the setting as its
/// component so slider changes can be routed back into the draft.
pub(super) fn project_drafted_graphics_detail_settings_into_authored_rows(
    mut commands: Commands,
    draft: Res<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    rows: Query<(Entity, Ref<UiListRow>)>,
    lists: Query<&UiListPolicy>,
    parents: Query<&ChildOf>,
    localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut texts: Query<(Entity, &UiTextBinding, &mut Text)>,
    mut sliders: Query<(Entity, &UiNodeId, &mut UiMaximum, &mut UiValue)>,
) {
    let Some(localization) = localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let slider_node = AssetId::from_key("ui/role/fragment/node/slider");
    for (row_entity, row) in &rows {
        if !lists.get(row.list).is_ok_and(|policy| {
            policy.source == UiWidgetLiveCollectionSource::GraphicsDetailSettings
        }) {
            continue;
        }
        let Some(&setting) = GraphicsDetailSetting::ALL.get(usize::from(row.index)) else {
            continue;
        };
        let level = graphics_detail_setting_level(setting, &draft.graphics);
        let level_keys = setting.level_localization_keys();
        let label = localization
            .find_plain_localized_text(AssetId::from_key(setting.label_localization_key()))
            .unwrap_or_default();
        let level_label = level_keys
            .get(usize::from(level))
            .and_then(|key| localization.find_plain_localized_text(AssetId::from_key(key)))
            .unwrap_or_default();
        let ancestors = |entity: Entity| {
            std::iter::successors(Some(entity), |entity| {
                parents.get(*entity).ok().map(ChildOf::parent)
            })
        };
        for (slider, node_id, mut maximum, mut value) in &mut sliders {
            if node_id.id == slider_node && ancestors(slider).any(|entity| entity == row_entity) {
                maximum.set_if_neq(UiMaximum(level_keys.len() as i64 - 1));
                value.set_if_neq(UiValue(i64::from(level)));
                commands.entity(slider).insert(setting);
            }
        }
        for (text_entity, binding, mut text) in &mut texts {
            let next = match binding.0 {
                UiTextPropertyBindingSource::GraphicsDetailSettingLabel => label,
                UiTextPropertyBindingSource::GraphicsDetailSettingLevel => level_label,
                _ => continue,
            };
            if text.0 != next && ancestors(text_entity).any(|entity| entity == row_entity) {
                next.clone_into(&mut text.0);
            }
        }
    }
}

/// Keeps slider changes local to the options draft, like the presets. The
/// accepted settings change only when the authored accept action runs.
pub(super) fn copy_changed_graphics_detail_slider_levels_into_settings_draft(
    mut activations: MessageReader<UiNodeActivated>,
    sliders: Query<(&GraphicsDetailSetting, &UiValue)>,
    mut draft: ResMut<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
) {
    for activation in activations.read() {
        if activation.trigger != UiTrigger::Change {
            continue;
        }
        let Ok((&setting, value)) = sliders.get(activation.node) else {
            continue;
        };
        let level = u8::try_from(value.0).unwrap_or(0);
        if graphics_detail_setting_level(setting, &draft.graphics) == level {
            continue;
        }
        set_graphics_detail_setting_level(setting, level, &mut draft.graphics);
        draft.dirty_graphics = true;
    }
}
