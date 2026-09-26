use smol_str::SmolStr;

use super::{blue_fang_source_document_format::BlueFangSourceDocumentFormat, path::AssetPath};

/// One transient parsed Blue Fang source document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OrderedSourceDocument {
    pub(crate) path: AssetPath,
    pub(crate) format: BlueFangSourceDocumentFormat,
    pub(crate) root: OrderedSourceDocumentNode,
}

/// Half-open byte range in the repaired, decoded UTF-8 document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct OrderedSourceDocumentSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Ordered source element. Repeated siblings remain repeated and in source order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OrderedSourceDocumentNode {
    pub(crate) name: SmolStr,
    pub(crate) attributes: Vec<OrderedSourceDocumentAttribute>,
    pub(crate) children: OrderedSourceDocumentChildren,
    pub(crate) span: OrderedSourceDocumentSpan,
}

impl OrderedSourceDocumentNode {
    pub(crate) fn attribute_named_any(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|wanted| {
            self.attributes.iter().find(|attribute| {
                super::source_document_semantic_name::source_document_names_are_semantically_equal(attribute.name(), wanted)
            }).map(|attribute| attribute.value())
        })
    }

    pub(crate) fn attribute_names(&self) -> impl Iterator<Item = &str> {
        self.attributes.iter().map(|attribute| attribute.name())
    }

    pub(crate) fn attributes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.attributes
            .iter()
            .map(|attribute| (attribute.name(), attribute.value()))
    }

    pub(crate) fn new_synthetic_ordered_source_document_node(
        name: SmolStr,
        attributes: Vec<OrderedSourceDocumentAttribute>,
        children: OrderedSourceDocumentChildren,
        span: OrderedSourceDocumentSpan,
    ) -> Self {
        Self {
            name,
            attributes,
            children,
            span,
        }
    }

    pub(crate) fn element_children(&self) -> OrderedSourceDocumentElementChildren<'_> {
        self.children.iter()
    }

    pub(crate) fn child_items(&self) -> &[OrderedSourceDocumentChild] {
        self.children.items()
    }

    pub(crate) fn first_text(&self) -> Option<&str> {
        self.children.first_text()
    }

    pub(crate) fn attribute(&self, attribute_name: &str) -> Option<&str> {
        find_ordered_source_document_attribute(&self.attributes, attribute_name)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OrderedSourceDocumentChildren {
    items: Vec<OrderedSourceDocumentChild>,
}

impl OrderedSourceDocumentChildren {
    pub(crate) fn from_ordered_source_document_child_items(
        items: Vec<OrderedSourceDocumentChild>,
    ) -> Self {
        Self { items }
    }

    pub(crate) fn items(&self) -> &[OrderedSourceDocumentChild] {
        &self.items
    }

    pub(crate) fn iter(&self) -> OrderedSourceDocumentElementChildren<'_> {
        OrderedSourceDocumentElementChildren {
            inner: self.items.iter(),
        }
    }

    pub(crate) fn first_text(&self) -> Option<&str> {
        self.items
            .iter()
            .find_map(OrderedSourceDocumentChild::as_text)
    }
}

impl<'a> IntoIterator for &'a OrderedSourceDocumentChildren {
    type Item = &'a OrderedSourceDocumentNode;
    type IntoIter = OrderedSourceDocumentElementChildren<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OrderedSourceDocumentElementChildren<'a> {
    inner: std::slice::Iter<'a, OrderedSourceDocumentChild>,
}

impl<'a> Iterator for OrderedSourceDocumentElementChildren<'a> {
    type Item = &'a OrderedSourceDocumentNode;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.find_map(OrderedSourceDocumentChild::as_element)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.inner.len()))
    }
}

impl DoubleEndedIterator for OrderedSourceDocumentElementChildren<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .by_ref()
            .rev()
            .find_map(OrderedSourceDocumentChild::as_element)
    }
}

/// Ordered child stream retained until typed asset lowering completes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OrderedSourceDocumentChild {
    Text(OrderedSourceDocumentText),
    Element(Box<OrderedSourceDocumentNode>),
    Comment,
    ProcessingInstruction,
}

impl OrderedSourceDocumentChild {
    pub(crate) fn as_element(&self) -> Option<&OrderedSourceDocumentNode> {
        match self {
            Self::Element(source_node) => Some(source_node.as_ref()),
            _ => None,
        }
    }

    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(source_text) => Some(&source_text.value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrderedSourceDocumentText {
    pub(crate) value: SmolStr,
}

impl OrderedSourceDocumentText {
    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrderedSourceDocumentAttribute {
    pub(crate) name: SmolStr,
    pub(crate) value: SmolStr,
}

impl OrderedSourceDocumentAttribute {
    pub(crate) fn new_synthetic_ordered_source_document_attribute(
        name: impl Into<SmolStr>,
        value: impl Into<SmolStr>,
    ) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn set_attribute_value(&mut self, value: impl Into<SmolStr>) {
        self.value = value.into();
    }
}

fn find_ordered_source_document_attribute<'a>(
    attributes: &'a [OrderedSourceDocumentAttribute],
    attribute_name: &str,
) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name == attribute_name)
        .map(|attribute| attribute.value.as_str())
}
