use std::io;

use openzt2_game_data::{
    localization::{LocalizationEntry, LocalizationRichContentKind, LocalizationRichContentNode},
    AssetId,
};

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentChild, OrderedSourceDocumentNode,
};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;

use super::{
    blue_fang_localization_format_tokenization::tokenize_blue_fang_localized_text_format,
    blue_fang_localization_presentation_lowering::{
        blue_fang_rich_content_kind, lower_blue_fang_rich_content_presentation,
        lower_localized_text_presentation,
    },
    blue_fang_localization_texture_path_normalization::normalize_blue_fang_localization_texture_path,
};

pub(super) fn lower_blue_fang_localization_entries(
    source_document: &OrderedSourceDocument,
) -> io::Result<Vec<LocalizationEntry>> {
    let mut localization_entries = Vec::new();
    for source_node in source_document.root.element_children() {
        lower_localization_entries_beneath_source_node(
            source_document,
            source_node,
            &mut localization_entries,
        )?;
    }
    Ok(localization_entries)
}

fn lower_localization_entries_beneath_source_node(
    source_document: &OrderedSourceDocument,
    source_node: &OrderedSourceDocumentNode,
    localization_entries: &mut Vec<LocalizationEntry>,
) -> io::Result<()> {
    if source_node.name.eq_ignore_ascii_case("LOC_STRING") {
        localization_entries.push(lower_localization_entry(source_document, source_node)?);
        return Ok(());
    }
    for child_node in source_node.element_children() {
        lower_localization_entries_beneath_source_node(
            source_document,
            child_node,
            localization_entries,
        )?;
    }
    Ok(())
}

fn lower_localization_entry(
    source_document: &OrderedSourceDocument,
    source_node: &OrderedSourceDocumentNode,
) -> io::Result<LocalizationEntry> {
    let authored_localization_key = source_node
        .attribute("_locID")
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} contains LOC_STRING without _locID",
                    source_document.path.as_str()
                ),
            )
        })?;
    let mut localization_entry_builder = LocalizationEntryBuilder::new(authored_localization_key);
    append_localization_source_children(
        source_node.child_items(),
        &mut localization_entry_builder,
        None,
    )?;
    localization_entry_builder.finish()
}

fn append_localization_source_children(
    source_children: &[OrderedSourceDocumentChild],
    localization_entry_builder: &mut LocalizationEntryBuilder,
    parent_rich_content_node_index: Option<u32>,
) -> io::Result<()> {
    for source_child in source_children {
        match source_child {
            OrderedSourceDocumentChild::Text(source_text) => append_localization_source_text(
                localization_entry_builder,
                parent_rich_content_node_index,
                source_text.value(),
            ),
            OrderedSourceDocumentChild::Element(source_node) => {
                let rich_content_node_index = u32::try_from(
                    localization_entry_builder
                        .localized_rich_content_nodes
                        .len(),
                )
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "localized rich content contains more than u32::MAX nodes",
                    )
                })?;
                let image_asset_path = source_node
                    .attribute("image")
                    .or_else(|| source_node.attribute("texture"))
                    .or_else(|| source_node.attribute("bgimg"))
                    .or_else(|| source_node.attribute("src"))
                    .filter(|path| !path.trim().is_empty())
                    .map(normalize_blue_fang_localization_texture_path);
                let linked_subject_path = source_node
                    .attribute("link")
                    .or_else(|| source_node.attribute("href"))
                    .map(str::to_owned);
                if matches!(
                    blue_fang_rich_content_kind(&source_node.name),
                    LocalizationRichContentKind::LineBreak
                ) {
                    localization_entry_builder.plain_localized_text.push('\n');
                    if source_node.name.eq_ignore_ascii_case("p") {
                        localization_entry_builder.plain_localized_text.push('\n');
                    }
                }
                localization_entry_builder
                    .localized_rich_content_nodes
                    .push(LocalizationRichContentNode {
                        parent_rich_content_node_index,
                        rich_content_kind: blue_fang_rich_content_kind(&source_node.name),
                        localized_text: String::new(),
                        image_asset_path,
                        linked_subject_path,
                        rich_content_presentation: lower_blue_fang_rich_content_presentation(
                            source_node,
                        ),
                    });
                append_localization_source_children(
                    source_node.child_items(),
                    localization_entry_builder,
                    Some(rich_content_node_index),
                )?;
            }
            OrderedSourceDocumentChild::Comment
            | OrderedSourceDocumentChild::ProcessingInstruction => {}
        }
    }
    Ok(())
}

fn append_localization_source_text(
    localization_entry_builder: &mut LocalizationEntryBuilder,
    parent_rich_content_node_index: Option<u32>,
    source_text: &str,
) {
    append_localization_text_with_formatting_whitespace_normalized(
        &mut localization_entry_builder.plain_localized_text,
        source_text,
    );
    let mut localized_text = String::new();
    append_localization_text_with_formatting_whitespace_normalized(
        &mut localized_text,
        source_text,
    );
    if !localized_text.is_empty() {
        if source_text
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
            && !localized_text
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            localized_text.push(' ');
        }
        // Mixed content stays in source order. Appending text to its element
        // moves labels around nested emphasis and loses text directly in cells.
        localization_entry_builder
            .localized_rich_content_nodes
            .push(LocalizationRichContentNode {
                parent_rich_content_node_index,
                rich_content_kind: LocalizationRichContentKind::Text,
                localized_text,
                image_asset_path: None,
                linked_subject_path: None,
                rich_content_presentation: Default::default(),
            });
    }
}

fn append_localization_text_with_formatting_whitespace_normalized(
    localized_text: &mut String,
    source_text: &str,
) {
    if source_text
        .bytes()
        .any(|byte| matches!(byte, b'\n' | b'\r' | b'\t'))
    {
        let mut source_lines = source_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let Some(first_source_line) = source_lines.next() else {
            return;
        };
        if !localized_text.is_empty()
            && !localized_text
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace)
        {
            localized_text.push(' ');
        }
        localized_text.push_str(first_source_line);
        for source_line in source_lines {
            localized_text.push(' ');
            localized_text.push_str(source_line);
        }
    } else {
        localized_text.push_str(source_text);
    }
}

struct LocalizationEntryBuilder {
    authored_localization_key: String,
    plain_localized_text: String,
    localized_rich_content_nodes: Vec<LocalizationRichContentNode>,
}

impl LocalizationEntryBuilder {
    fn new(authored_localization_key: String) -> Self {
        Self {
            authored_localization_key,
            plain_localized_text: String::new(),
            localized_rich_content_nodes: Vec::new(),
        }
    }

    fn finish(self) -> io::Result<LocalizationEntry> {
        let localized_text_presentation =
            lower_localized_text_presentation(&self.localized_rich_content_nodes);
        Ok(LocalizationEntry {
            localization_entry_identifier: AssetId::from_key(
                &canonicalize_source_document_record_key(&self.authored_localization_key),
            ),
            authored_localization_key: self.authored_localization_key,
            localized_text_format_tokens: tokenize_blue_fang_localized_text_format(
                &self.plain_localized_text,
            )?,
            plain_localized_text: self.plain_localized_text,
            localized_rich_content_nodes: self.localized_rich_content_nodes,
            localized_text_presentation,
        })
    }
}

#[cfg(test)]
mod tests {
    use openzt2_game_data::localization::LocalizationRichContentKind;

    use super::lower_blue_fang_localization_entries;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    #[test]
    fn zoopedia_labels_emphasis_and_breaks_retain_source_order() {
        let source = parse_blue_fang_source_document(AssetPath::new("lang/1033/animal_entries.xml"),
            br#"<ZT2Strings><LOC_STRING _locID="zoopedia_tiger:text"><cell width="284" pady="10"><color r="255" g="255" b="255">Class: Mammals <i>(Mammalia)</i><br/>Order: Carnivores <i>(Carnivora)</i><br/>Family: Cats</color></cell></LOC_STRING></ZT2Strings>"#).expect("authored rich text parses");
        let entries = lower_blue_fang_localization_entries(&source).expect("rich text lowers");
        let records = &entries[0].localized_rich_content_nodes;
        let ordered_text: String = records
            .iter()
            .map(|record| match record.rich_content_kind {
                LocalizationRichContentKind::LineBreak => "\n",
                _ => record.localized_text.as_str(),
            })
            .collect();
        assert_eq!(
            ordered_text,
            "Class: Mammals (Mammalia)\nOrder: Carnivores (Carnivora)\nFamily: Cats"
        );
        assert_eq!(
            records[0].rich_content_presentation.padding_pixels,
            Some([0.0, 10.0])
        );
        let italic_leaf = records
            .iter()
            .find(|record| record.localized_text == "(Mammalia)")
            .expect("italic leaf");
        let parent = &records[italic_leaf
            .parent_rich_content_node_index
            .expect("italic parent") as usize];
        assert!(parent.rich_content_presentation.italic_text);
    }
}
