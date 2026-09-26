use bevy::{
    prelude::*,
    sprite::{BorderRect, SliceScaleMode, TextureSlicer},
};
use openzt2_game_data::{
    ui_document::{
        document::UiDocument,
        node_layout::{UiNodeRegionDefinition, UiNodeRegionMetric},
        node_presentation::{
            UiNodePointerHitPolicy, UiNodeVisualState, UiNodeVisualStateDefinition,
        },
        node_property_binding::{
            UiImagePropertyBindingSource, UiNodePropertyBinding, UiTextPropertyBindingSource,
        },
        node_style::{UiStyleFlags, UiStyleRecord, UiSystemFont, UiTextAlignment},
    },
    AssetId,
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::plugins::ui::authored_ui_text_layout_presentation::{
    UiPhysicalFontSize, UiSingleLineTextAlignment, AUTHORED_COMIC_SANS_COMPATIBLE_FONT_FAMILY,
};
use crate::plugins::ui::authored_ui_visual_types::{
    UiSourceRect, UiVisualImageColor, UiVisualLayer, UiVisualPicking, UiVisualSound,
    UiVisualTextColor,
};
use crate::plugins::ui::localized_ui_text_writing::write_localized_ui_text;

pub(super) fn project_authored_node_visuals_into_bevy_presentation(
    commands: &mut Commands,
    entity: Entity,
    data: &UiDocument,
    node_id: AssetId,
    bindings: &[UiNodePropertyBinding],
    visuals: &[UiNodeVisualStateDefinition],
    literal_text: &str,
    text_format: AssetId,
    style: &UiStyleRecord,
    text_color_style: &UiStyleRecord,
    document: &UiDocumentAsset,
    localization: crate::assets::localization::loaded_localization_queries::LoadedLocalizationView<
        '_,
    >,
    enabled: bool,
    image_selection_index: Option<usize>,
    runtime_text: Option<String>,
) {
    let mut background =
        selected_image(data, node_id, image_selection_index).unwrap_or(AssetId(style.background.0));
    let mut key = AssetId(style.text_key.0);
    let mut has_text_binding = false;
    let mut has_dynamic_image_binding = false;
    for binding in bindings {
        match binding.clone() {
            UiNodePropertyBinding::ImageContent(UiImagePropertyBindingSource::Constant {
                texture,
            }) => background = texture,
            UiNodePropertyBinding::ImageContent(_) => has_dynamic_image_binding = true,
            UiNodePropertyBinding::TextContent(source) => {
                has_text_binding = true;
                if let UiTextPropertyBindingSource::Localized {
                    key: localized_key, ..
                } = source
                {
                    key = localized_key;
                }
            }
            _ => {}
        }
    }
    let hit_visual = visuals
        .iter()
        .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Normal))
        .or_else(|| {
            visuals
                .iter()
                .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Default))
        });
    if let Some(visual) =
        hit_visual.filter(|visual| matches!(&visual.hit_policy, UiNodePointerHitPolicy::Normal))
    {
        let authored_image = AssetId(visual.image.0);
        let image_id = if authored_image == AssetId::default() {
            background
        } else {
            authored_image
        };
        if let Some(metadata) = document.cloned_interactive_texture_metadata_handle(image_id) {
            commands
                .entity(entity)
                .insert(crate::plugins::ui::picking::UiAlphaHitTest {
                    metadata,
                    source_rect: visual_source_rect(&visual.source_rect),
                });
        }
    }
    let presentation_key = if text_format == AssetId::default() {
        key
    } else {
        text_format
    };
    let formatter_owns_color = (presentation_key != AssetId::default())
        .then(|| localization.find_localized_text_presentation(presentation_key))
        .flatten()
        .is_some_and(|presentation| presentation.text_color_rgba.is_some());
    let node_has_text_presentation = runtime_text.is_some()
        || has_text_binding
        || !literal_text.is_empty()
        || presentation_key != AssetId::default();
    // `style.background` is the default image identity from the authored
    // aspect. When the same aspect has visual-state rows, those child layers
    // own both the image and its source rectangle. Rendering the default again
    // on the parent draws the entire atlas beneath every cropped button and is
    // the source of the overlapping icon sheets seen in-game.
    if visuals.is_empty() && background != AssetId::default() {
        if let Some(image) = document.cloned_texture_image_handle(background) {
            let source = style.background_source.map(|value| value);
            let mut image_node = ImageNode::new(image);
            // A BF UIRegion is the destination rectangle. BFRect only selects
            // the sampled source rectangle; it does not request aspect-fit
            // sizing. Bevy's `Auto` mode preserves the texture aspect ratio,
            // which shrinks authored non-square destinations such as the
            // photo-album book and many panel skins.
            image_node.image_mode = NodeImageMode::Stretch;
            if source[2] > 0 && source[3] > 0 {
                image_node.rect = Some(Rect {
                    min: Vec2::new(source[0] as f32, source[1] as f32),
                    max: Vec2::new(
                        (source[0] + source[2]) as f32,
                        (source[1] + source[3]) as f32,
                    ),
                });
            }
            commands.entity(entity).insert(image_node);
        }
    }

    let initial_role = visuals
        .iter()
        .find(|visual| {
            if enabled {
                matches!(&visual.visual_state, UiNodeVisualState::Normal)
            } else {
                matches!(&visual.visual_state, UiNodeVisualState::Disabled)
            }
        })
        .map(|_| {
            if enabled {
                UiNodeVisualState::Normal
            } else {
                UiNodeVisualState::Disabled
            }
        })
        .or_else(|| {
            visuals
                .iter()
                .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Normal))
                .map(|_| UiNodeVisualState::Normal)
        })
        .or_else(|| {
            visuals
                .iter()
                .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Default))
                .map(|_| UiNodeVisualState::Default)
        });
    let initial_text_color = visuals
        .iter()
        .find(|visual| {
            (visual.affects_text_color
                || node_has_text_presentation
                    && AssetId(visual.image.0) == AssetId::default()
                    && visual.affects_color)
                && if enabled {
                    matches!(&visual.visual_state, UiNodeVisualState::Normal)
                } else {
                    matches!(&visual.visual_state, UiNodeVisualState::Disabled)
                }
        })
        .or_else(|| {
            (!enabled).then_some(()).and_then(|()| {
                visuals.iter().find(|visual| {
                    (visual.affects_text_color
                        || node_has_text_presentation
                            && AssetId(visual.image.0) == AssetId::default()
                            && visual.affects_color)
                        && matches!(&visual.visual_state, UiNodeVisualState::Normal)
                })
            })
        })
        .or_else(|| {
            visuals.iter().find(|visual| {
                (visual.affects_text_color
                    || node_has_text_presentation
                        && AssetId(visual.image.0) == AssetId::default()
                        && visual.affects_color)
                    && !formatter_owns_color
                    && matches!(&visual.visual_state, UiNodeVisualState::Default)
            })
        })
        .map(|visual| {
            Color::srgba(
                visual.text_color[0],
                visual.text_color[1],
                visual.text_color[2],
                visual.text_color[3],
            )
        });
    if has_dynamic_image_binding {
        if let Some(visual) = visuals.iter().find(|visual| {
            Some(visual.visual_state) == initial_role
                && visual.affects_color
                && AssetId(visual.image.0) == AssetId::default()
                && background == AssetId::default()
        }) {
            let color = Color::srgba(
                visual.color[0],
                visual.color[1],
                visual.color[2],
                visual.color[3],
            );
            commands
                .entity(entity)
                .entry::<ImageNode>()
                .and_modify(move |mut image| image.color = color);
        }
    }
    for (offset, visual) in visuals.iter().enumerate() {
        let role = visual.visual_state;
        let color = Color::srgba(
            visual.color[0],
            visual.color[1],
            visual.color[2],
            visual.color[3],
        );
        // File- and directory-skin selections replace an authored visual whose
        // source image is intentionally empty. Source lowering has already reduced
        // the source skin to one canonical texture identity; use that identity
        // for the visual layer instead of dropping it merely because this node
        // has explicit normal/highlighted state rows.
        let authored_image = AssetId(visual.image.0);
        let image_id = if authored_image == AssetId::default() {
            background
        } else {
            authored_image
        };
        let source_rect = visual_source_rect(&visual.source_rect);
        // A dynamic binding owns the sampled texture. Authored image identity
        // is only its initial placeholder and must not remain as a child layer
        // over the runtime-bound image. The visual still owns state, tint,
        // sound, and picking presentation.
        let image = (!has_dynamic_image_binding)
            .then(|| document.cloned_texture_image_handle(image_id))
            .flatten();
        // Older source documents serialized an authored `<default/>` as a white visual.
        // It is a declaration placeholder, not a drawable. Source lowering
        // omits these records; do not even spawn the compatibility record,
        // otherwise its required BackgroundColor can be animated opaque.
        let empty_visual = source_rect.is_none()
            && (!visual.affects_color || color.alpha() <= 0.0)
            && !visual.affects_text_color
            && AssetId(visual.sound.0) == AssetId::default()
            && matches!(&visual.hit_policy, UiNodePointerHitPolicy::Normal)
            && image.is_none();
        if empty_visual {
            continue;
        }
        let mut layer_commands = commands.spawn((
            Name::new(format!("ui visual {offset}")),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            },
            UiVisualLayer(role),
            UiVisualPicking(!matches!(&visual.hit_policy, UiNodePointerHitPolicy::Never)),
            // Sort skins before authored child controls. Render extraction
            // assigns these aspect draws to the owner's background slot;
            // negative child ZIndex alone cannot place them behind parent text.
            ZIndex(-1),
            if Some(role) == initial_role {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            Pickable::IGNORE,
            ChildOf(entity),
        ));
        let text_only_aspect_color =
            node_has_text_presentation && image.is_none() && visual.affects_color;
        if (visual.affects_text_color || text_only_aspect_color)
            && (!formatter_owns_color
                || !matches!(&visual.visual_state, UiNodeVisualState::Default))
        {
            layer_commands.insert(UiVisualTextColor(if visual.affects_text_color {
                Color::srgba(
                    visual.text_color[0],
                    visual.text_color[1],
                    visual.text_color[2],
                    visual.text_color[3],
                )
            } else {
                color
            }));
        }
        if has_dynamic_image_binding && image.is_none() && visual.affects_color {
            layer_commands.insert(UiVisualImageColor(color));
        }
        let layer = layer_commands.id();
        if let Some(rect) = source_rect {
            commands.entity(layer).insert(UiSourceRect([
                rect.min.x as i32,
                rect.min.y as i32,
                rect.width() as i32,
                rect.height() as i32,
            ]));
        }
        if let Some(image) = image {
            let mut image_node = ImageNode::new(image);
            image_node.color = color;
            image_node.rect = source_rect;
            image_node.image_mode = NodeImageMode::Stretch;
            if style.flags.0 & UiStyleFlags::NINE_SLICE.0 != 0 {
                let border = style.border.map(|value| value);
                image_node.image_mode = NodeImageMode::Sliced(TextureSlicer {
                    border: BorderRect::from(border),
                    center_scale_mode: SliceScaleMode::Stretch,
                    sides_scale_mode: SliceScaleMode::Stretch,
                    max_corner_scale: 1.0,
                });
            }
            // An image visual is composited solely from its sampled texture.
            // Keep the Node's required background transparent: an opaque fill
            // here destroys authored alpha (notably the main-menu pointer
            // atlas) and turns a sparse sprite into a solid rectangle.
            commands
                .entity(layer)
                .insert((image_node, BackgroundColor(Color::NONE)));
        }
        let sound = AssetId(visual.sound.0);
        if let Some(sound) = document.audio_source_handle(sound) {
            commands.entity(layer).insert(UiVisualSound(sound.clone()));
        }
    }

    let mut text = runtime_text.unwrap_or_default();
    if text.is_empty() {
        if key != AssetId::default() {
            let _ = write_localized_ui_text(localization, key, &[], &mut text);
        } else if let Some(value) = (!literal_text.is_empty()).then_some(literal_text) {
            text.push_str(value);
        }
    }
    // Empty bound fields still need Text and its formatting: list labels and
    // other dynamic values may arrive after projection.
    if text.is_empty() && !has_text_binding && text_format == AssetId::default() {
        return;
    }
    // `<text format>` owns typography independently of the text/locid that
    // supplies the displayed words. The original applies that formatter after
    // resolving dynamic values; using the value key here drops authored size,
    // weight, colour and shadow from every formatted field.
    let localized_presentation = (presentation_key != AssetId::default())
        .then(|| localization.find_localized_text_presentation(presentation_key))
        .flatten();
    let localized_text_color = localized_presentation
        .and_then(|value| value.text_color_rgba)
        .map(|color| Color::srgba_u8(color[0], color[1], color[2], color[3]));
    // The formatter supplies the base run style; the active UIAspect visual
    // is applied afterwards by the original widget and therefore owns any
    // state-specific text colour.
    let text_color = initial_text_color
        .or(localized_text_color)
        .unwrap_or_else(|| {
            Color::srgba(
                text_color_style.color[0],
                text_color_style.color[1],
                text_color_style.color[2],
                text_color_style.color[3],
            )
        });
    let font_size = localized_presentation
        .and_then(|value| value.font_size_pixels)
        .map_or_else(|| style.font_px, |value| value);
    let font_bold = localized_presentation.is_some_and(|value| value.bold_text) || style.font_bold;
    commands.entity(entity).insert((
        Text::new(text),
        TextFont {
            font: match &style.font {
                UiSystemFont::Arial => FontSource::SansSerif,
                UiSystemFont::ComicSansMs => {
                    FontSource::Family(AUTHORED_COMIC_SANS_COMPATIBLE_FONT_FAMILY.into())
                }
            },
            font_size: FontSize::Px(font_size),
            width: FontWidth::NORMAL,
            weight: if font_bold {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            ..default()
        },
        UiPhysicalFontSize::from_physical_pixels(font_size),
        TextColor(text_color),
        // Bevy owns run measurement, Unicode split boundaries, line grouping,
        // positioning and alignment. UI document's closed `multi` policy selects soft
        // wrapping; every other authored text block keeps only explicit line
        // breaks instead of silently acquiring Bevy's default wrapping.
        TextLayout::new(
            match &style.font_alignment {
                UiTextAlignment::Left => Justify::Left,
                UiTextAlignment::Center => Justify::Center,
                UiTextAlignment::Right => Justify::Right,
                UiTextAlignment::Justified => Justify::Justified,
            },
            if style.flags.0 & UiStyleFlags::WRAP_TEXT.0 != 0 {
                LineBreak::WordOrCharacter
            } else {
                LineBreak::NoWrap
            },
        ),
    ));
    if style.flags.0 & UiStyleFlags::WRAP_TEXT.0 == 0 {
        commands.entity(entity).insert(UiSingleLineTextAlignment);
    }
    if style.font_underline {
        commands.entity(entity).insert(Underline);
    }
    let shadow = localized_presentation
        .and_then(|value| value.shadow_offset_pixels)
        .map_or_else(
            || [style.font_shadow_offset[0], style.font_shadow_offset[1]],
            |value| [i32::from(value[0]), i32::from(value[1])],
        );
    if shadow != [0, 0] {
        commands.entity(entity).insert(TextShadow {
            offset: Vec2::new(shadow[0] as f32, shadow[1] as f32),
            ..default()
        });
    }
}

/// Resolve a source file-skin to one native texture without retaining a skin
/// manager or mutable selection registry. The document identity supplies a
/// stable choice, so every target in one authored replacement group receives
/// the same texture for that document instance.
fn selected_image(
    data: &UiDocument,
    node_id: AssetId,
    image_selection_index: Option<usize>,
) -> Option<AssetId> {
    data.image_selection_groups.iter().find_map(|selection| {
        selection
            .target_node_ids
            .iter()
            .any(|target| target.0 == node_id.0)
            .then(|| {
                let default_index =
                    || u64::from_le_bytes(data.id.0[..8].try_into().unwrap()) as usize;
                let index = image_selection_index.unwrap_or_else(default_index);
                AssetId(
                    selection.candidate_image_ids[index % selection.candidate_image_ids.len()].0,
                )
            })
    })
}

fn visual_source_rect(region: &UiNodeRegionDefinition) -> Option<Rect> {
    let pixel = |metric: &UiNodeRegionMetric| match metric {
        UiNodeRegionMetric::Pixels(value) => Some(*value),
        _ => None,
    };
    let x = pixel(&region.metrics[0])?;
    let y = pixel(&region.metrics[1])?;
    let width = pixel(&region.metrics[2])?;
    let height = pixel(&region.metrics[3])?;
    (width > 0.0 && height > 0.0).then_some(Rect {
        min: Vec2::new(x, y),
        max: Vec2::new(x + width, y + height),
    })
}
