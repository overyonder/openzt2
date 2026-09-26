//! Blue Fang visual inheritance and canonical style and visual-state lowering.

use crate::assets::source_document::ui::model::{
    SourceUiAspect, SourceUiFont, SourceUiHitPolicy, SourceUiNamedVisual, SourceUiNode,
    SourceUiVisual, SourceUiWidgetData,
};
use crate::assets::ui_document::source::lower::authored_ui_asset_dependency_resolution::{
    dependency, optional_resolved_audio_dependency_path, visual_texture_dependency,
};
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::{
    AuthoredUiDocument, SOURCE_POINTS_TO_PIXELS,
};
use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::{
    color4, invalid_at,
};
use crate::assets::ui_document::source::lower::authored_ui_template_resolution::{
    metric, system_font, text_alignment,
};
use crate::assets::ui_document::source::lower::authored_ui_widget_record_lowering::{
    hit_policy, visual_state,
};
use openzt2_game_data::ui_document::node_layout::UiNodeRegionDefinition;
use openzt2_game_data::ui_document::node_presentation::{
    UiNodePointerHitPolicy, UiNodeVisualState, UiNodeVisualStateDefinition,
};
use openzt2_game_data::ui_document::node_style::{UiStyleFlags, UiStyleRecord, UiSystemFont};
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_source_ui_node_style_to_canonical_style_record(
    node: &SourceUiNode,
    nine_slice: Option<[f32; 4]>,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<UiStyleRecord> {
    let Some(aspect) = &node.aspect else {
        let Some(background) = &node.background_layout else {
            return Ok(UiStyleRecord::default());
        };
        let mut style = UiStyleRecord::default();
        style.background = visual_texture_dependency(Some(background.image.key()), output, input)?;
        style.background_source = background.source;
        style.background_width = background.width.unwrap_or(0).max(0) as f32;
        style.padding = [
            background.padding[0] as f32,
            background.padding[1] as f32,
            0.0,
            0.0,
        ];
        return Ok(style);
    };
    let visual = aspect
        .default
        .as_ref()
        .or_else(|| aspect.standard.first().map(|value| &value.visual));
    let mut style = UiStyleRecord::default();
    if let Some(visual) = visual {
        if visual
            .font
            .as_ref()
            .and_then(|font| font.image.as_ref())
            .is_some()
        {
            return Err(invalid_at(
                input,
                "bitmap-font image has no native visual owner; original content uses system faces",
            ));
        }
        style.background =
            visual_texture_dependency(visual.image.as_ref().map(|path| path.key()), output, input)?;
        style.font = UiSystemFont::default();
        if let Some(color) = visual.font.as_ref().and_then(|font| font.color) {
            style.color = color4(color);
        }
        if let Some(font) = &visual.font {
            style.font = system_font(font.face.as_deref(), input)?;
            style.font_px = font
                .size
                .map_or(style.font_px, |size| size * SOURCE_POINTS_TO_PIXELS);
            style.font_offset = [font.x.unwrap_or(0), font.y.unwrap_or(0)];
            style.font_shadow_offset = [font.shadow_x.unwrap_or(0), font.shadow_y.unwrap_or(0)];
            style.font_bold = font.bold.unwrap_or(false);
            style.font_underline = font.underline.unwrap_or(false);
        }
        style.font_alignment = text_alignment(
            visual
                .font
                .as_ref()
                .and_then(|font| font.align.as_deref())
                .or(visual.text_alignment.as_deref()),
            input,
        )?;
        match visual.text_format.as_deref() {
            None => {}
            Some("multi") => style.flags = style.flags | UiStyleFlags::WRAP_TEXT,
            Some(value) => {
                return Err(invalid_at(
                    input,
                    format!("unsupported text layout format {value:?}"),
                ));
            }
        }
    }
    let text_type = match &node.widget {
        SourceUiWidgetData::Text(text) => text.text_type.as_deref(),
        SourceUiWidgetData::TextEdit { text, .. } => text.text_type.as_deref(),
        SourceUiWidgetData::Tooltip(tooltip) => tooltip.text.text_type.as_deref(),
        _ => None,
    };
    if text_type == Some("multi") {
        style.flags = style.flags | UiStyleFlags::WRAP_TEXT;
    }
    if let Some(key) = &aspect.localization_id {
        style.text_key = dependency(Some(key.clone()), output);
    }
    style.padding = [
        aspect.pad_x.unwrap_or(0) as f32,
        aspect.pad_y.unwrap_or(0) as f32,
        0.0,
        0.0,
    ];
    if let Some(background) = &node.background_layout {
        style.background = visual_texture_dependency(Some(background.image.key()), output, input)?;
        style.background_source = background.source;
        style.background_width = background.width.unwrap_or(0).max(0) as f32;
        style.padding[0] = background.padding[0] as f32;
        style.padding[1] = background.padding[1] as f32;
    }
    if aspect.draw_3d {
        style.flags = style.flags | UiStyleFlags::DRAW_3D;
    }
    if let Some(border) = nine_slice {
        style.flags = style.flags | UiStyleFlags::NINE_SLICE;
        style.border = border;
    }
    Ok(style)
}

pub(super) fn lower_source_ui_node_visuals_to_canonical_visual_state_definitions(
    node: &SourceUiNode,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<()> {
    let Some(aspect) = &node.aspect else {
        return Ok(());
    };
    let inherited_hit_policy = node.always_hit.as_ref().unwrap_or(&aspect.hit_policy);
    // Blue Fang's `default` visual is the base recipe for the named states,
    // not an additional drawable state.  In particular, the main-menu paws
    // put the shared atlas path on `default` and only the atlas rectangle on
    // each named state.  Serializing those fragments independently displays
    // the entire atlas for normal and produces image-less hover states.
    if aspect.standard.is_empty() && aspect.alternate.is_empty() {
        if let Some(visual) = aspect.default.as_ref().filter(|visual| {
            visual.image.is_some()
                || visual.color.is_some()
                || visual.font.is_some()
                || visual.sound.is_some()
                || visual.rect.is_some()
        }) {
            push_visual(
                UiNodeVisualState::Default,
                visual,
                inherited_hit_policy,
                output,
                input,
            )?;
        }
        return Ok(());
    }
    for visual in &aspect.standard {
        let effective = merge_visual(aspect.default.as_ref(), &visual.visual);
        push_visual(
            visual_state(&visual.name, false, input)?,
            &effective,
            inherited_hit_policy,
            output,
            input,
        )?;
    }
    for visual in &aspect.alternate {
        let standard = aspect
            .standard
            .iter()
            .find(|candidate| candidate.name == visual.name)
            .map(|candidate| &candidate.visual);
        let base = standard
            .map(|standard| merge_visual(aspect.default.as_ref(), standard))
            .or_else(|| aspect.default.clone());
        let effective = merge_visual(base.as_ref(), &visual.visual);
        push_visual(
            visual_state(&visual.name, true, input)?,
            &effective,
            inherited_hit_policy,
            output,
            input,
        )?;
    }
    Ok(())
}

fn merge_visual(base: Option<&SourceUiVisual>, overlay: &SourceUiVisual) -> SourceUiVisual {
    let Some(base) = base else {
        return overlay.clone();
    };
    SourceUiVisual {
        // An empty image attribute is how authored instances retain the
        // reusable template's skin while overriding the rest of the visual.
        // Treat it as an omitted overlay value; compiling it as an actual
        // empty asset erases `blank`, `header`, and other inherited skins.
        image: overlay
            .image
            .as_ref()
            .filter(|image| !image.is_empty())
            .cloned()
            .or_else(|| base.image.clone()),
        sound: overlay.sound.clone().or_else(|| base.sound.clone()),
        hit_policy: overlay
            .hit_policy
            .clone()
            .or_else(|| base.hit_policy.clone()),
        rect: overlay.rect.clone().or_else(|| base.rect.clone()),
        color: overlay.color.or(base.color),
        font: match (&base.font, &overlay.font) {
            (base, Some(authored)) => {
                let mut font = merge_font(base.as_ref(), authored);
                // A state-local generic BFColor is the presentation colour
                // for an image-free text control. It supersedes only a font
                // colour inherited from the default or corresponding standard
                // state; a BFFont colour authored on this state remains more
                // specific. `locationbutton` uses exactly this form for its
                // white selected rows over amber normal text.
                if overlay.color.is_some() && authored.color.is_none() {
                    font.color = None;
                }
                Some(font)
            }
            (Some(base), None) => {
                let mut font = base.clone();
                if overlay.color.is_some() {
                    font.color = None;
                }
                Some(font)
            }
            (None, None) => None,
        },
        text_alignment: overlay
            .text_alignment
            .clone()
            .or_else(|| base.text_alignment.clone()),
        text_format: overlay
            .text_format
            .clone()
            .or_else(|| base.text_format.clone()),
        unknown_attributes: overlay.unknown_attributes.clone(),
        unknown_elements: overlay.unknown_elements.clone(),
    }
}

fn merge_font(base: Option<&SourceUiFont>, overlay: &SourceUiFont) -> SourceUiFont {
    let Some(base) = base else {
        return overlay.clone();
    };
    SourceUiFont {
        face: overlay.face.clone().or_else(|| base.face.clone()),
        size: overlay.size.or(base.size),
        align: overlay.align.clone().or_else(|| base.align.clone()),
        x: overlay.x.or(base.x),
        y: overlay.y.or(base.y),
        shadow_x: overlay.shadow_x.or(base.shadow_x),
        shadow_y: overlay.shadow_y.or(base.shadow_y),
        bold: overlay.bold.or(base.bold),
        underline: overlay.underline.or(base.underline),
        color: overlay.color.or(base.color),
        image: overlay.image.clone().or_else(|| base.image.clone()),
        unknown_attributes: base
            .unknown_attributes
            .iter()
            .chain(&overlay.unknown_attributes)
            .cloned()
            .collect(),
    }
}

pub(super) fn merge_source_ui_aspect_template_and_authored_override(
    base: Option<&SourceUiAspect>,
    overlay: &SourceUiAspect,
) -> SourceUiAspect {
    let Some(base) = base else {
        return overlay.clone();
    };
    let merge_named = |base: &[SourceUiNamedVisual], overlay: &[SourceUiNamedVisual]| {
        let mut merged = base.to_vec();
        for authored in overlay {
            if let Some(existing) = merged
                .iter_mut()
                .find(|candidate| candidate.name == authored.name)
            {
                existing.visual = merge_visual(Some(&existing.visual), &authored.visual);
            } else {
                merged.push(authored.clone());
            }
        }
        merged
    };
    SourceUiAspect {
        localization_id: overlay
            .localization_id
            .clone()
            .or_else(|| base.localization_id.clone()),
        authored_string: overlay
            .authored_string
            .clone()
            .or_else(|| base.authored_string.clone()),
        hit_policy: if overlay.hit_policy == SourceUiHitPolicy::Normal {
            base.hit_policy.clone()
        } else {
            overlay.hit_policy.clone()
        },
        pad_x: overlay.pad_x.or(base.pad_x),
        pad_y: overlay.pad_y.or(base.pad_y),
        draw_3d: overlay.draw_3d || base.draw_3d,
        auto_size: overlay.auto_size.or(base.auto_size),
        default: match (&base.default, &overlay.default) {
            (base, Some(authored)) => Some(merge_visual(base.as_ref(), authored)),
            (Some(base), None) => Some(base.clone()),
            (None, None) => None,
        },
        standard: merge_named(&base.standard, &overlay.standard),
        alternate: merge_named(&base.alternate, &overlay.alternate),
        unknown_attributes: base
            .unknown_attributes
            .iter()
            .chain(&overlay.unknown_attributes)
            .cloned()
            .collect(),
        unknown_elements: base
            .unknown_elements
            .iter()
            .chain(&overlay.unknown_elements)
            .cloned()
            .collect(),
    }
}

fn push_visual(
    state: UiNodeVisualState,
    visual: &SourceUiVisual,
    inherited_hit_policy: &SourceUiHitPolicy,
    output: &mut BuildOutput,
    input: &AuthoredUiDocument,
) -> io::Result<()> {
    if visual
        .font
        .as_ref()
        .and_then(|font| font.image.as_ref())
        .is_some()
    {
        return Err(invalid_at(
            input,
            "bitmap-font image has no native visual owner; original content uses system faces",
        ));
    }
    let image =
        visual_texture_dependency(visual.image.as_ref().map(|path| path.key()), output, input)?;
    let sound = visual
        .sound
        .as_deref()
        .map(|name| optional_resolved_audio_dependency_path(name, input))
        .transpose()?
        .flatten()
        .map_or_else(AssetId::default, |path| dependency(Some(path), output));
    let font = AssetId::default();
    let source_rect = if let Some(rect) = &visual.rect {
        UiNodeRegionDefinition {
            metrics: [
                metric(&rect.x, input)?,
                metric(&rect.y, input)?,
                metric(&rect.width, input)?,
                metric(&rect.height, input)?,
            ],
            ..UiNodeRegionDefinition::default()
        }
    } else {
        UiNodeRegionDefinition::default()
    };
    let hit_policy = hit_policy(
        Some(visual.hit_policy.as_ref().unwrap_or(inherited_hit_policy)),
        input,
    )?;
    if hit_policy == UiNodePointerHitPolicy::Normal && image != AssetId::default() {
        output.interactive_textures.insert(image.0);
    }
    output.visuals.push(UiNodeVisualStateDefinition {
        visual_state: state,
        image,
        sound,
        source_rect,
        color: visual.color.map(color4).unwrap_or([1.0; 4]),
        affects_color: visual.color.is_some(),
        text_color: visual
            .font
            .as_ref()
            .and_then(|font| font.color)
            .or(visual.color)
            .map(color4)
            .unwrap_or([1.0; 4]),
        affects_text_color: visual
            .font
            .as_ref()
            .and_then(|font| font.color)
            .or(visual.color)
            .is_some(),
        font,
        hit_policy,
    });
    Ok(())
}

#[cfg(test)]
mod hit_policy_tests {
    use crate::assets::source_document::blue_fang_source_document_parsing::parse_blue_fang_source_document;
    use crate::assets::source_document::path::AssetPath;
    use crate::assets::source_document::ui::parser::SourceUiDocument;
    use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
    use crate::assets::ui_document::source::lower::authored_ui_node_tree_lowering::BuildOutput;
    use crate::assets::ui_document::source::lower::visuals::lower_source_ui_node_visuals_to_canonical_visual_state_definitions;
    use openzt2_game_data::ui_document::document::UiDocumentRole;
    use openzt2_game_data::ui_document::node_presentation::UiNodePointerHitPolicy;
    use openzt2_game_data::AssetId;

    #[test]
    fn exposure_visual_states_inherit_aspect_hit_policy() {
        let parsed = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/exposure.xml"),
            br#"<UIToggleButton name="frame"><UIAspect alwayshit="true">
                <default><BFColor r="255" g="255" b="255" a="255"/></default>
                <standard><normal/><highlighted alwayshit="never"/></standard>
                <alternate><normal/></alternate>
            </UIAspect></UIToggleButton>"#,
        )
        .unwrap();
        let input = AuthoredUiDocument {
            virtual_path: "ui/layout/exposure.xml".into(),
            id: AssetId::default(),
            role: UiDocumentRole::Fragment,
            source: SourceUiDocument::parse(&parsed),
            templates: Default::default(),
            image_selections: Vec::new(),
            interaction_cursors: None,
            placement_preview: None,
            role_targets: Default::default(),
            resolved_dependencies: Default::default(),
        };
        let mut output = BuildOutput::default();
        lower_source_ui_node_visuals_to_canonical_visual_state_definitions(
            &input.source.root,
            &mut output,
            &input,
        )
        .unwrap();
        assert_eq!(
            output
                .visuals
                .iter()
                .map(|visual| visual.hit_policy)
                .collect::<Vec<_>>(),
            vec![
                UiNodePointerHitPolicy::Always,
                UiNodePointerHitPolicy::Never,
                UiNodePointerHitPolicy::Always
            ]
        );
    }
}
