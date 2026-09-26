//! Lossless copying of retained unknown UI source payload nodes.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    super::model::SourceUiPayloadNode,
    source_scalar_and_attribute_reading::copy_source_ui_attribute,
};

impl SourceUiPayloadNode {
    pub(super) fn from_data_node(node: &OrderedSourceDocumentNode) -> Self {
        Self {
            name: node.name.to_string(),
            attributes: node
                .attributes
                .iter()
                .map(copy_source_ui_attribute)
                .collect(),
            children: node.element_children().map(Self::from_data_node).collect(),
            text: node.first_text().map(str::to_owned),
            span: node.span,
        }
    }
}
