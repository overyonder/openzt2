use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        loaded_localization_queries::LoadedLocalizationView,
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::localization::LocalizationFormatArgument;
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::{UiDocumentMembers, UiNodeId},
    ui_document_lifecycle_contracts::ShowUiDocument,
};

use super::authored_ui_text_layout_presentation::{
    resolve_authored_font_family_to_bevy_font_source, UiPhysicalFontSize,
};
use super::graph_generated_geometry::GraphTick;
use super::projection::authored_document_projection::project_document;

#[derive(Component)]
pub(super) struct UiGraphTickLabel {
    document: Handle<UiDocumentAsset>,
    root: Entity,
    value: f64,
    bottom: f32,
    width: f32,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_graph_tick_labels(
    commands: &mut Commands,
    graph: Entity,
    children: Option<&Children>,
    instances: &mut Query<&mut UiGraphTickLabel>,
    handle: Option<&Handle<UiDocumentAsset>>,
    documents: &Assets<UiDocumentAsset>,
    localization: LoadedLocalizationView<'_>,
    ticks: Vec<GraphTick>,
    width: f32,
) {
    let mut existing = children
        .into_iter()
        .flatten()
        .copied()
        .filter(|entity| instances.contains(*entity))
        .collect::<Vec<_>>()
        .into_iter();
    let Some((handle, document)) = handle.and_then(|handle| Some((handle, documents.get(handle)?)))
    else {
        for child in existing {
            commands.entity(child).try_despawn();
        }
        return;
    };
    for tick in ticks {
        if let Some(child) = existing.next() {
            if let Ok(mut instance) = instances.get_mut(child) {
                if &instance.document == handle {
                    instance
                        .reborrow()
                        .map_unchanged(|instance| &mut instance.value)
                        .set_if_neq(tick.value);
                    instance
                        .reborrow()
                        .map_unchanged(|instance| &mut instance.bottom)
                        .set_if_neq(tick.bottom);
                    instance
                        .map_unchanged(|instance| &mut instance.width)
                        .set_if_neq(width);
                    continue;
                }
            }
            commands.entity(child).try_despawn();
        }
        let slot = commands
            .spawn((
                ChildOf(graph),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    bottom: px(tick.bottom),
                    width: px(width),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .id();
        let root = project_document(
            commands,
            &ShowUiDocument {
                document: handle.clone(),
                owner: slot,
            },
            document,
            localization,
            None,
            false,
        );
        commands.entity(slot).insert(UiGraphTickLabel {
            document: handle.clone(),
            root,
            value: tick.value,
            bottom: tick.bottom,
            width,
        });
    }
    for child in existing {
        commands.entity(child).try_despawn();
    }
}

pub(super) fn write_graph_tick_text(
    mut ticks: Query<(Ref<UiGraphTickLabel>, &mut Node)>,
    documents: Res<Assets<UiDocumentAsset>>,
    members: Query<&UiDocumentMembers>,
    mut texts: Query<(
        &UiNodeId,
        &mut Text,
        &mut TextColor,
        &mut TextFont,
        &mut UiPhysicalFontSize,
    )>,
    index: Res<LocalizationPrecedenceIndex>,
    locales: Res<Assets<LocalizationAsset>>,
) {
    let refresh = index.is_changed() || locales.is_changed() || documents.is_changed();
    let Some(localization) = index.borrow_loaded_localization_view(&locales) else {
        return;
    };
    for (tick, mut slot) in &mut ticks {
        if !tick.is_changed() && !refresh {
            continue;
        }
        slot.reborrow()
            .map_unchanged(|slot| &mut slot.bottom)
            .set_if_neq(px(tick.bottom));
        slot.map_unchanged(|slot| &mut slot.width)
            .set_if_neq(px(tick.width));
        let (Some(document), Ok(nodes)) = (documents.get(&tick.document), members.get(tick.root))
        else {
            continue;
        };
        for entity in nodes.iter() {
            let Ok((node, text, mut color, mut font, mut physical_size)) = texts.get_mut(entity)
            else {
                continue;
            };
            let Some(label) = document.graph_label_text_node(node.index) else {
                continue;
            };
            let mut value = String::new();
            let key = if let Some(format) = label.money {
                let cents = (tick.value * 100.0).round() as i64;
                let amount = if format.omit_minus {
                    cents.saturating_abs()
                } else {
                    cents
                };
                if localization
                    .write_localized_currency_amount(amount, !format.omit_cents, &mut value)
                    .is_err()
                {
                    continue;
                }
                if cents < 0 {
                    format.negative
                } else {
                    format.positive
                }
            } else {
                if tick.value.fract().abs() < f64::EPSILON {
                    value = format!("{:.0}", tick.value);
                } else {
                    value = format!("{:.1}", tick.value);
                }
                label.format
            };
            let mut formatted = String::new();
            if localization
                .write_localized_text_with_format_arguments(
                    key,
                    &[LocalizationFormatArgument::Text(&value)],
                    &mut formatted,
                )
                .is_err()
            {
                formatted = value;
            }
            let selected_presentation = localization.find_localized_text_presentation(key);
            let template_presentation = localization.find_localized_text_presentation(label.format);
            let specified_color =
                |presentation: &openzt2_game_data::localization::LocalizationTextPresentation| {
                    presentation.text_color_rgba
                };
            let rgba = selected_presentation
                .and_then(specified_color)
                .or_else(|| template_presentation.and_then(specified_color))
                .unwrap_or(label.color);
            if let Some(presentation) = selected_presentation.or(template_presentation) {
                if let Some(family) = &presentation.font_family {
                    let desired = resolve_authored_font_family_to_bevy_font_source(family);
                    font.reborrow()
                        .map_unchanged(|font| &mut font.font)
                        .set_if_neq(desired);
                }
                let weight = if presentation.bold_text {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                };
                font.reborrow()
                    .map_unchanged(|font| &mut font.weight)
                    .set_if_neq(weight);
                if let Some(size) = presentation.font_size_pixels {
                    if physical_size.physical_pixels() != size {
                        *physical_size = UiPhysicalFontSize::from_physical_pixels(size);
                    }
                    font.map_unchanged(|font| &mut font.font_size)
                        .set_if_neq(FontSize::Px(size));
                }
            }
            color.set_if_neq(TextColor(Color::srgba_u8(
                rgba[0], rgba[1], rgba[2], rgba[3],
            )));
            text.map_unchanged(|text| &mut text.0).set_if_neq(formatted);
        }
    }
}
