use bevy::{ecs::system::EntityCommands, prelude::*};
use openzt2_game_assets::{
    graphics_settings_presets::{GraphicsSettingsPresetSource, GraphicsSettingsPresets},
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
};
use openzt2_game_data::ui_document::{
    node_property_binding::UiTextPropertyBindingSource,
    widget_live_collection::UiWidgetLiveCollectionSource,
};
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        RequestedUiListRowCount, UiListPolicy, UiListRow,
    },
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_node_projection_components::{UiSliderValueChanged, UiValue},
};
use openzt2_ui::{
    authored_tooltip_presentation::{
        UiDisplayNameKey, UiHelpTopicKey, UiLongTooltipKey, UiShortTooltipKey,
    },
    authored_ui_text_content_binding::UiTextBinding,
    slider::UiSliderPolicy,
};

use super::options_screen_settings_draft_types::OptionsScreenDisplayGraphicsAndOnlineMessageDraft;

pub(super) fn set_authored_graphics_detail_setting_list_row_count(
    source: Res<GraphicsSettingsPresetSource>,
    assets: Res<Assets<GraphicsSettingsPresets>>,
    mut lists: Query<(&UiListPolicy, &mut RequestedUiListRowCount)>,
) {
    let Some(settings) = source.get(&assets) else {
        return;
    };
    let count = u16::try_from(settings.detail_rows().len()).unwrap_or(u16::MAX);
    for (policy, mut requested) in &mut lists {
        if policy.source == UiWidgetLiveCollectionSource::GraphicsDetailSettings {
            requested.set_if_neq(RequestedUiListRowCount(Some(count)));
        }
    }
}

pub(super) fn project_drafted_graphics_detail_settings_into_authored_rows(
    mut commands: Commands,
    source: Res<GraphicsSettingsPresetSource>,
    assets: Res<Assets<GraphicsSettingsPresets>>,
    drafts: Query<Ref<OptionsScreenDisplayGraphicsAndOnlineMessageDraft>>,
    rows: Query<(Entity, Ref<UiListRow>, &ChildOf)>,
    lists: Query<&UiListPolicy>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut texts: Query<(&UiTextBinding, &mut Text)>,
    mut sliders: Query<(
        &mut UiSliderPolicy,
        &mut UiMinimum,
        &mut UiMaximum,
        &mut UiValue,
    )>,
    mut names: Query<&mut Name>,
    mut help: Query<(
        Option<&mut UiDisplayNameKey>,
        Option<&mut UiShortTooltipKey>,
        Option<&mut UiLongTooltipKey>,
        Option<&mut UiHelpTopicKey>,
    )>,
) {
    let refresh = source.is_changed()
        || assets.is_changed()
        || localization.is_changed()
        || localization_assets.is_changed();
    let Some(settings) = source.get(&assets) else {
        return;
    };
    let localization = localization.borrow_loaded_localization_view(&localization_assets);
    for (entity, row, parent) in &rows {
        if !is_graphics_detail_list(parent.parent(), &lists) {
            continue;
        }
        let Some(draft) = parents
            .iter_ancestors::<ChildOf>(entity)
            .find_map(|ancestor| drafts.get(ancestor).ok())
        else {
            continue;
        };
        if !refresh && !row.is_changed() && !draft.is_changed() {
            continue;
        }
        let Some(setting) = settings.detail_rows().get(usize::from(row.index)) else {
            continue;
        };
        let value = setting.selected_value(&draft.graphics);
        for descendant in std::iter::once(entity).chain(children.iter_descendants(entity)) {
            if let Ok((policy, mut minimum, mut maximum, mut slider_value)) =
                sliders.get_mut(descendant)
            {
                policy
                    .map_unchanged(|policy| &mut policy.increment)
                    .set_if_neq(setting.increment);
                minimum.set_if_neq(UiMinimum(setting.minimum));
                maximum.set_if_neq(UiMaximum(setting.maximum));
                slider_value.set_if_neq(UiValue(value));
            }
            if let Ok((binding, mut text)) = texts.get_mut(descendant) {
                let key = match binding.0 {
                    UiTextPropertyBindingSource::GraphicsDetailSettingLabel => setting.label,
                    UiTextPropertyBindingSource::GraphicsDetailSettingLevel => {
                        if let Ok(mut name) = names.get_mut(descendant) {
                            if !setting.setting_widget.is_empty()
                                && name.as_str() != setting.setting_widget.as_str()
                            {
                                name.set(setting.setting_widget.clone());
                            }
                        }
                        setting.value_label(value)
                    }
                    _ => continue,
                };
                let next = localization
                    .and_then(|localization| localization.find_plain_localized_text(key))
                    .unwrap_or_default();
                if text.0 != next {
                    next.clone_into(&mut text.0);
                }
            }
            let (Ok(name), Ok((name_key, short_key, long_key, topic_key))) =
                (names.get(descendant), help.get_mut(descendant))
            else {
                continue;
            };
            let definition = match name.as_str() {
                name if name.eq_ignore_ascii_case("thumb") => &setting.thumb_help,
                name if name.eq_ignore_ascii_case("slideup") => &setting.up_help,
                name if name.eq_ignore_ascii_case("slidedown") => &setting.down_help,
                _ => continue,
            };
            let mut node = commands.entity(descendant);
            project_help_key(&mut node, name_key, UiDisplayNameKey(definition.name));
            project_help_key(&mut node, short_key, UiShortTooltipKey(definition.short));
            project_help_key(&mut node, long_key, UiLongTooltipKey(definition.long));
            project_help_key(&mut node, topic_key, UiHelpTopicKey(definition.help));
        }
    }
}

pub(super) fn apply_accepted_graphics_detail_slider_edits_to_draft(
    mut edits: MessageReader<UiSliderValueChanged>,
    source: Res<GraphicsSettingsPresetSource>,
    assets: Res<Assets<GraphicsSettingsPresets>>,
    rows: Query<(&UiListRow, &ChildOf)>,
    lists: Query<&UiListPolicy>,
    mut drafts: Query<&mut OptionsScreenDisplayGraphicsAndOnlineMessageDraft>,
    parents: Query<&ChildOf>,
) {
    let Some(settings) = source.get(&assets) else {
        edits.clear();
        return;
    };
    for edit in edits.read() {
        let mut ancestors = parents.iter_ancestors::<ChildOf>(edit.slider);
        let Some((row, _)) = ancestors
            .find_map(|ancestor| rows.get(ancestor).ok())
            .filter(|(_, parent)| is_graphics_detail_list(parent.parent(), &lists))
        else {
            continue;
        };
        let Some(setting) = settings.detail_rows().get(usize::from(row.index)) else {
            continue;
        };
        let Some(owner) = ancestors.find(|owner| drafts.contains(*owner)) else {
            continue;
        };
        let Ok(mut draft) = drafts.get_mut(owner) else {
            continue;
        };
        let mut next = draft.graphics;
        setting.apply_value(edit.value, &mut next);
        if draft.graphics != next {
            draft.graphics = next;
            draft.dirty_graphics = true;
        }
    }
}

fn is_graphics_detail_list(entity: Entity, lists: &Query<&UiListPolicy>) -> bool {
    lists
        .get(entity)
        .is_ok_and(|policy| policy.source == UiWidgetLiveCollectionSource::GraphicsDetailSettings)
}

fn project_help_key<K: Component + Default + PartialEq>(
    node: &mut EntityCommands<'_>,
    current: Option<Mut<'_, K>>,
    next: K,
) {
    if let Some(mut current) = current {
        current.set_if_neq(next);
    } else if next != K::default() {
        node.insert(next);
    }
}
