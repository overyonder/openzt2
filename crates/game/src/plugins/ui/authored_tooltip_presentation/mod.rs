use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use openzt2_game_data::{
    localization::LocalizationRichContentKind, ui_document::widget_control::UiTooltipPresentation,
    AssetId,
};

use super::authored_ui_canvas_scaling_and_clipping::UiLogicalCanvas;
use super::picking::UiPointerCapture;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiTooltipPolicy {
    pub(crate) presentation: UiTooltipPresentation,
    pub(crate) global: bool,
    pub(crate) floating: bool,
    pub(crate) autohide: bool,
    pub(crate) offset: [i32; 2],
    pub(crate) appear_seconds: f32,
    pub(crate) display_seconds: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiDisplayNameKey(pub(crate) AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiLongTooltipKey(pub(crate) AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiShortTooltipKey(pub(crate) AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiHelpTopicKey(pub(crate) AssetId);

#[derive(Component)]
pub(super) struct UiTooltipTextPresentation {
    tooltip: Entity,
    key: AssetId,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct UiTooltipState {
    target: Option<Entity>,
    hovered_seconds: f32,
    visible_seconds: f32,
}

pub(super) fn update_authored_tooltip_presentations(
    mut commands: Commands,
    time: Res<Time>,
    capture: Res<UiPointerCapture>,
    active_localization: Res<
        crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex,
    >,
    localizations: Res<
        Assets<crate::assets::localization::localization_asset_types::LocalizationAsset>,
    >,
    mut tooltips: Query<(
        Entity,
        &UiTooltipPolicy,
        &crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner,
        Option<&mut UiTooltipState>,
        &mut Visibility,
        &mut Text,
        &TextFont,
        &mut TextColor,
        &TextLayout,
        &mut Node,
        &ComputedNode,
    )>,
    tooltip_text_presentations: Query<(Entity, &UiTooltipTextPresentation)>,
    help_nodes: Query<(
        Option<&UiDisplayNameKey>,
        Option<&UiShortTooltipKey>,
        Option<&UiLongTooltipKey>,
        Option<&UiHelpTopicKey>,
        Option<&ChildOf>,
        &crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner,
    )>,
    windows: Query<&Window, With<PrimaryWindow>>,
    canvases: Query<(&UiLogicalCanvas, &UiTransform)>,
) {
    let localization = active_localization.borrow_loaded_localization_view(&localizations);
    for (
        entity,
        policy,
        owner,
        mut current_state,
        mut visibility,
        mut text,
        font,
        mut text_color,
        text_layout,
        mut node,
        computed,
    ) in &mut tooltips
    {
        let previous_state = current_state.as_deref().copied();
        let mut state = previous_state.unwrap_or_default();
        let hovered =
            hovered_help_target(capture.target, owner.0, entity, policy.global, &help_nodes);
        let key = hovered.and_then(|hovered| {
            tooltip_key_from_ancestry(
                hovered,
                (!policy.global).then_some(owner.0),
                policy.presentation,
                &help_nodes,
            )
        });
        if let Some((localization, key, label)) = key.and_then(|key| {
            localization.and_then(|localization| {
                crate::plugins::ui::localized_ui_text_writing::localized_ui_text(localization, key)
                    .map(|label| (localization, key, label))
            })
        }) {
            let changed_target = state.target != hovered;
            if changed_target {
                state = UiTooltipState {
                    target: hovered,
                    ..default()
                };
            }
            let has_current_text_presentation = tooltip_text_presentations
                .iter()
                .any(|(_, presentation)| presentation.tooltip == entity && presentation.key == key);
            if changed_target || !has_current_text_presentation {
                set_tooltip_text(
                    &mut commands,
                    entity,
                    &mut text,
                    font,
                    &mut text_color,
                    text_layout,
                    node.padding,
                    policy.floating,
                    localization,
                    key,
                    label,
                    &tooltip_text_presentations,
                );
            }
            if policy.floating {
                if let Some((window, position)) = windows
                    .iter()
                    .next()
                    .and_then(|window| window.cursor_position().map(|position| (window, position)))
                {
                    node.position_type = PositionType::Absolute;
                    let viewport = Vec2::new(window.width(), window.height());
                    let (logical_size, transform) = canvases
                        .get(owner.0)
                        .map_or((viewport, UiTransform::IDENTITY), |(canvas, transform)| {
                            (canvas.logical_size(), *transform)
                        });
                    let scale = transform.scale.max(Vec2::splat(f32::EPSILON));
                    let translation = Vec2::new(
                        match transform.translation.x {
                            Val::Px(value) => value,
                            _ => 0.0,
                        },
                        match transform.translation.y {
                            Val::Px(value) => value,
                            _ => 0.0,
                        },
                    );
                    let origin = logical_size * 0.5 + translation - logical_size * scale * 0.5;
                    let visible_min = -origin / scale;
                    let visible_max = (viewport - origin) / scale;
                    let size = computed.size() * computed.inverse_scale_factor();
                    let desired = (position - origin) / scale
                        + Vec2::new(policy.offset[0] as f32, policy.offset[1] as f32);
                    let maximum = (visible_max - size).max(visible_min);
                    let clamped = desired.clamp(visible_min, maximum);
                    node.left = px(clamped.x);
                    node.top = px(clamped.y);
                }
            }
            state.hovered_seconds += time.delta_secs();
            state.visible_seconds = if state.hovered_seconds >= policy.appear_seconds {
                state.visible_seconds + time.delta_secs()
            } else {
                0.0
            };
        } else {
            state = UiTooltipState::default();
            text.0.clear();
            retire_tooltip_text_presentations(&mut commands, entity, &tooltip_text_presentations);
        }
        let timed_out = policy.autohide
            && policy.display_seconds > 0.0
            && state.visible_seconds > policy.display_seconds;
        let next_visibility = if state.hovered_seconds >= policy.appear_seconds && !timed_out {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
        if let Some(current_state) = current_state.as_deref_mut() {
            if *current_state != state {
                *current_state = state;
            }
        } else if state != UiTooltipState::default() {
            commands.entity(entity).insert(state);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn set_tooltip_text(
    commands: &mut Commands,
    tooltip: Entity,
    text: &mut Text,
    base_font: &TextFont,
    base_text_color: &mut TextColor,
    base_text_layout: &TextLayout,
    tooltip_text_padding: UiRect,
    floating: bool,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    key: AssetId,
    fallback: &str,
    tooltip_text_presentations: &Query<(Entity, &UiTooltipTextPresentation)>,
) {
    retire_tooltip_text_presentations(commands, tooltip, tooltip_text_presentations);
    text.0.clear();
    text.0.push_str(fallback);
    *base_text_color = TextColor(Color::NONE);
    let content = localization
        .find_localized_rich_content(key)
        .filter(|content| !content.is_empty());
    let tooltip_text_presentation_entity = commands
        .spawn((
            UiTooltipTextPresentation { tooltip, key },
            Text::new(content.map_or(fallback, |_| "")),
            base_font.clone(),
            TextColor(Color::WHITE),
            base_text_layout.clone(),
            if floating {
                // A Taffy node with children does not use its own text measure.
                // Keep floating tooltip text in normal flow so it supplies the
                // auto-sized parent extent; the absolute skin children then
                // follow that canonical layout result.
                Node::default()
            } else {
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    width: percent(100.0),
                    height: percent(100.0),
                    padding: tooltip_text_padding,
                    ..default()
                }
            },
            ZIndex(1),
            Pickable::IGNORE,
            ChildOf(tooltip),
        ))
        .id();
    let Some(content) = content else {
        return;
    };

    // Localized tooltip entries contain their heading, body, hotkey, line
    // breaks, and per-run presentation. The old path used only the data's
    // flattened fallback string, which discarded the authored colors and
    // emphasis even though source lowering had preserved them.
    for record in content {
        let value = match record.rich_content_kind {
            LocalizationRichContentKind::Text => Some(record.localized_text.clone()),
            LocalizationRichContentKind::LineBreak => {
                let mut line_break_and_contained_text =
                    String::with_capacity(record.localized_text.len() + 1);
                line_break_and_contained_text.push('\n');
                line_break_and_contained_text.push_str(&record.localized_text);
                Some(line_break_and_contained_text)
            }
            LocalizationRichContentKind::Cell | LocalizationRichContentKind::Image => None,
        };
        let Some(value) = value else {
            continue;
        };
        let mut font_size_pixels = record.rich_content_presentation.font_size_pixels;
        let mut text_color_rgba = record.rich_content_presentation.text_color_rgba;
        let mut bold_text = record.rich_content_presentation.bold_text;
        let mut italic_text = record.rich_content_presentation.italic_text;
        let mut underlined_text = record.rich_content_presentation.underlined_text;
        let mut parent_record_index = record.parent_rich_content_node_index;
        while let Some(parent_record) = parent_record_index
            .and_then(|parent_record_index| content.get(parent_record_index as usize))
        {
            font_size_pixels =
                font_size_pixels.or(parent_record.rich_content_presentation.font_size_pixels);
            text_color_rgba =
                text_color_rgba.or(parent_record.rich_content_presentation.text_color_rgba);
            bold_text |= parent_record.rich_content_presentation.bold_text;
            italic_text |= parent_record.rich_content_presentation.italic_text;
            underlined_text |= parent_record.rich_content_presentation.underlined_text;
            parent_record_index = parent_record.parent_rich_content_node_index;
        }
        let mut font = base_font.clone();
        if let Some(font_size) = font_size_pixels {
            font.font_size = FontSize::Px(font_size);
        }
        font.weight = if bold_text {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };
        font.style = if italic_text {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        let color = if let Some(color_rgba) = text_color_rgba {
            TextColor(Color::srgba_u8(
                color_rgba[0],
                color_rgba[1],
                color_rgba[2],
                color_rgba[3],
            ))
        } else {
            TextColor(Color::WHITE)
        };
        let mut span = commands.spawn((
            TextSpan::new(value),
            font,
            color,
            ChildOf(tooltip_text_presentation_entity),
        ));
        if underlined_text {
            span.insert(Underline);
        }
    }
}

fn retire_tooltip_text_presentations(
    commands: &mut Commands,
    tooltip: Entity,
    tooltip_text_presentations: &Query<(Entity, &UiTooltipTextPresentation)>,
) {
    tooltip_text_presentations
        .iter()
        .filter(|(_, presentation)| presentation.tooltip == tooltip)
        .for_each(|(entity, _)| {
            commands.entity(entity).despawn();
        });
}

fn hovered_help_target(
    target: Option<Entity>,
    owner: Entity,
    tooltip: Entity,
    global: bool,
    help_nodes: &Query<(
        Option<&UiDisplayNameKey>,
        Option<&UiShortTooltipKey>,
        Option<&UiLongTooltipKey>,
        Option<&UiHelpTopicKey>,
        Option<&ChildOf>,
        &crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner,
    )>,
) -> Option<Entity> {
    let target = target?;
    (target != tooltip)
        .then(|| help_nodes.get(target).ok())
        .flatten()
        .filter(|(_, _, _, _, _, candidate_owner)| global || candidate_owner.0 == owner)
        .map(|_| target)
}

fn tooltip_key_from_ancestry(
    mut entity: Entity,
    required_owner: Option<Entity>,
    presentation: UiTooltipPresentation,
    help_nodes: &Query<(
        Option<&UiDisplayNameKey>,
        Option<&UiShortTooltipKey>,
        Option<&UiLongTooltipKey>,
        Option<&UiHelpTopicKey>,
        Option<&ChildOf>,
        &crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner,
    )>,
) -> Option<AssetId> {
    let (_, _, _, _, _, owner) = help_nodes.get(entity).ok()?;
    let target_owner = owner.0;
    if required_owner.is_some_and(|owner| owner != target_owner) {
        return None;
    }
    loop {
        let (name, short, long, help, parent, candidate_owner) = help_nodes.get(entity).ok()?;
        if candidate_owner.0 != target_owner {
            return None;
        }
        let key = match presentation {
            UiTooltipPresentation::Name => name.map(|key| key.0),
            UiTooltipPresentation::Short => short.map(|key| key.0),
            UiTooltipPresentation::Long => long.map(|key| key.0),
            UiTooltipPresentation::Help => help.map(|key| key.0),
        };
        if key.is_some() || entity == target_owner {
            return key;
        }
        entity = parent?.parent();
    }
}
