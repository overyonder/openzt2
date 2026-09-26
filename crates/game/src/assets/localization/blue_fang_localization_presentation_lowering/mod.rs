use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::localization::{
    LocalizationRichContentKind, LocalizationRichContentNode, LocalizationRichContentPresentation,
    LocalizationTextPresentation,
};

use super::blue_fang_localization_texture_path_normalization::normalize_blue_fang_localization_texture_path;

pub(super) fn lower_localized_text_presentation(
    rich_content_nodes: &[LocalizationRichContentNode],
) -> LocalizationTextPresentation {
    let text_color_rgba = rich_content_nodes
        .iter()
        .find_map(|node| node.rich_content_presentation.text_color_rgba);
    let font_size_pixels = rich_content_nodes
        .iter()
        .find_map(|node| node.rich_content_presentation.font_size_pixels);
    let bold_text = rich_content_nodes
        .iter()
        .any(|node| node.rich_content_presentation.bold_text);
    let mut preceding_width = 0.0;
    let mut content_offset = None;
    let mut background_image = None;
    let mut background_offset = [0.0; 2];
    let mut background_extent = [0.0; 2];
    for node in rich_content_nodes
        .iter()
        .filter(|node| matches!(node.rich_content_kind, LocalizationRichContentKind::Cell))
    {
        if let Some(image_asset_path) = node
            .image_asset_path
            .as_deref()
            .filter(|image_asset_path| !image_asset_path.trim().is_empty())
        {
            background_image = Some(normalize_blue_fang_localization_texture_path(
                image_asset_path,
            ));
            background_offset = [preceding_width, 0.0];
            background_extent = [
                node.rich_content_presentation
                    .width_pixels
                    .unwrap_or_default(),
                node.rich_content_presentation
                    .height_pixels
                    .unwrap_or_default(),
            ];
            break;
        }
        if let Some(width) = node.rich_content_presentation.width_pixels {
            preceding_width = width;
        }
        if let Some(padding) = node.rich_content_presentation.padding_pixels {
            content_offset = Some([preceding_width + padding[0], padding[1]]);
        }
    }
    LocalizationTextPresentation {
        text_color_rgba,
        font_size_pixels,
        bold_text,
        shadow_offset_pixels: None,
        content_offset_pixels: content_offset,
        background_image_asset_path: background_image,
        background_offset_pixels: background_offset,
        background_extent_pixels: background_extent,
    }
}

pub(super) fn lower_blue_fang_rich_content_presentation(
    node: &OrderedSourceDocumentNode,
) -> LocalizationRichContentPresentation {
    let number = |name| node.attribute(name).and_then(|value| value.parse().ok());
    let width = number("width");
    let height = number("height");
    let padding_x = number("padx");
    let padding_y = number("pady");
    let source_x = number("sx");
    let source_y = number("sy");
    let source_width = number("sw");
    let source_height = number("sh");
    let channel = |name| node.attribute(name).and_then(|value| value.parse().ok());
    let text_color_rgba = match (channel("r"), channel("g"), channel("b"), channel("a")) {
        (Some(red), Some(green), Some(blue), alpha) => {
            Some([red, green, blue, alpha.unwrap_or(255)])
        }
        _ => None,
    };
    LocalizationRichContentPresentation {
        width_pixels: width.or(source_width),
        height_pixels: height.or(source_height),
        padding_pixels: (padding_x.is_some() || padding_y.is_some())
            .then(|| [padding_x.unwrap_or(0.0), padding_y.unwrap_or(0.0)]),
        source_rectangle_pixels: source_x
            .zip(source_y)
            .zip(source_width.zip(source_height))
            .map(|((x, y), (width, height))| [x, y, width, height]),
        text_color_rgba,
        font_size_pixels: number("size").map(|size| size * (96.0 / 72.0)),
        bold_text: node.name.eq_ignore_ascii_case("b"),
        italic_text: node.name.eq_ignore_ascii_case("i"),
        underlined_text: node.name.eq_ignore_ascii_case("u") || node.name.eq_ignore_ascii_case("a"),
        starts_paragraph: node.name.eq_ignore_ascii_case("p"),
    }
}

pub(super) fn blue_fang_rich_content_kind(name: &str) -> LocalizationRichContentKind {
    if name.eq_ignore_ascii_case("cell") {
        LocalizationRichContentKind::Cell
    } else if name.eq_ignore_ascii_case("image") || name.eq_ignore_ascii_case("img") {
        LocalizationRichContentKind::Image
    } else if name.eq_ignore_ascii_case("br")
        || name.eq_ignore_ascii_case("linebreak")
        || name.eq_ignore_ascii_case("p")
    {
        LocalizationRichContentKind::LineBreak
    } else {
        LocalizationRichContentKind::Text
    }
}
