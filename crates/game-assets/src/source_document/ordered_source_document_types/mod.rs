use smol_str::SmolStr;

use super::{blue_fang_source_document_format::BlueFangSourceDocumentFormat, path::AssetPath};

/// One transient parsed Blue Fang source document.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedSourceDocument {
    pub path: AssetPath,
    pub format: BlueFangSourceDocumentFormat,
    pub root: OrderedSourceDocumentNode,
}

/// Half-open byte range in the repaired, decoded UTF-8 document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrderedSourceDocumentSpan {
    pub start: usize,
    pub end: usize,
}

/// Ordered source element. Repeated siblings remain repeated and in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedSourceDocumentNode {
    pub name: SmolStr,
    pub attributes: Vec<OrderedSourceDocumentAttribute>,
    pub children: OrderedSourceDocumentChildren,
    pub span: OrderedSourceDocumentSpan,
}

impl OrderedSourceDocumentNode {
    pub fn attribute_named_any(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|wanted| {
            self.attributes.iter().find(|attribute| {
                super::source_document_semantic_name::source_document_names_are_semantically_equal(attribute.name(), wanted)
            }).map(|attribute| attribute.value())
        })
    }

    pub fn attribute_names(&self) -> impl Iterator<Item = &str> {
        self.attributes.iter().map(|attribute| attribute.name())
    }

    pub fn attributes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.attributes
            .iter()
            .map(|attribute| (attribute.name(), attribute.value()))
    }

    pub fn new_synthetic_ordered_source_document_node(
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

    pub fn element_children(&self) -> OrderedSourceDocumentElementChildren<'_> {
        self.children.iter()
    }

    pub fn child_items(&self) -> &[OrderedSourceDocumentChild] {
        self.children.items()
    }

    pub fn first_text(&self) -> Option<&str> {
        self.children.first_text()
    }

    pub fn attribute(&self, attribute_name: &str) -> Option<&str> {
        find_ordered_source_document_attribute(&self.attributes, attribute_name)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrderedSourceDocumentChildren {
    items: Vec<OrderedSourceDocumentChild>,
}

impl OrderedSourceDocumentChildren {
    pub fn from_ordered_source_document_child_items(
        items: Vec<OrderedSourceDocumentChild>,
    ) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[OrderedSourceDocumentChild] {
        &self.items
    }

    pub fn iter(&self) -> OrderedSourceDocumentElementChildren<'_> {
        OrderedSourceDocumentElementChildren {
            inner: self.items.iter(),
        }
    }

    pub fn first_text(&self) -> Option<&str> {
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
pub struct OrderedSourceDocumentElementChildren<'a> {
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
pub enum OrderedSourceDocumentChild {
    Text(OrderedSourceDocumentText),
    Element(Box<OrderedSourceDocumentNode>),
    Comment,
    ProcessingInstruction,
}

impl OrderedSourceDocumentChild {
    pub fn as_element(&self) -> Option<&OrderedSourceDocumentNode> {
        match self {
            Self::Element(source_node) => Some(source_node.as_ref()),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(source_text) => Some(&source_text.value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedSourceDocumentText {
    pub value: SmolStr,
}

impl OrderedSourceDocumentText {
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedSourceDocumentAttribute {
    pub name: SmolStr,
    pub value: SmolStr,
}

impl OrderedSourceDocumentAttribute {
    pub fn new_synthetic_ordered_source_document_attribute(
        name: impl Into<SmolStr>,
        value: impl Into<SmolStr>,
    ) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_attribute_value(&mut self, value: impl Into<SmolStr>) {
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
