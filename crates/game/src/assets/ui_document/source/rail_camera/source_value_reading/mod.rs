//! Typed scalar, vector, matrix, and tree reads from rail-camera source nodes.

use std::io;

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn required_source_attribute<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<&'a str> {
    source_node
        .attribute(attribute_name)
        .filter(|attribute_value| !attribute_value.trim().is_empty())
        .ok_or_else(|| {
            invalid_source_data(format!("<{}> requires {attribute_name}", source_node.name))
        })
}

pub(super) fn required_finite_source_number(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<f32> {
    parse_finite_source_number(required_source_attribute(source_node, attribute_name)?)
        .map_err(|_| invalid_source_data(format!("invalid {attribute_name}")))
}

pub(super) fn optional_source_boolean_attribute(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> io::Result<Option<bool>> {
    source_node
        .attribute(attribute_name)
        .map(
            |attribute_value| match attribute_value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(invalid_source_data(format!("invalid {attribute_name}"))),
            },
        )
        .transpose()
}

pub(super) fn source_element_child<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    child_name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    source_node
        .element_children()
        .find(|source_child| source_child.name.eq_ignore_ascii_case(child_name))
}

pub(super) fn find_source_descendant<'a>(
    source_roots: impl Iterator<Item = &'a OrderedSourceDocumentNode>,
    descendant_name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    source_roots.into_iter().find_map(|source_node| {
        source_node
            .name
            .eq_ignore_ascii_case(descendant_name)
            .then_some(source_node)
            .or_else(|| find_source_descendant(source_node.element_children(), descendant_name))
    })
}

pub(super) fn parse_finite_source_number(source_value: &str) -> io::Result<f32> {
    source_value
        .trim_end_matches(['f', 'F'])
        .parse::<f32>()
        .ok()
        .filter(|parsed_value| parsed_value.is_finite())
        .ok_or_else(|| invalid_source_data("invalid finite floating-point value"))
}

pub(super) fn read_source_vector_attributes(
    source_node: &OrderedSourceDocumentNode,
) -> io::Result<[f32; 3]> {
    Ok([
        required_finite_source_number(source_node, "x")?,
        required_finite_source_number(source_node, "y")?,
        required_finite_source_number(source_node, "z")?,
    ])
}

pub(super) fn read_source_vector_string(source_value: &str) -> io::Result<[f32; 3]> {
    source_value
        .split_ascii_whitespace()
        .map(parse_finite_source_number)
        .collect::<io::Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid_source_data("expected three vector components"))
}

pub(super) fn read_source_rotation_matrix(
    source_node: &OrderedSourceDocumentNode,
) -> io::Result<[f32; 9]> {
    let [source_row_0, source_row_1, source_row_2] = ["row0", "row1", "row2"].map(|row_name| {
        source_element_child(source_node, row_name)
            .ok_or_else(|| invalid_source_data("rotation is missing a matrix row"))
    });
    let (source_row_0, source_row_1, source_row_2) = (source_row_0?, source_row_1?, source_row_2?);
    Ok([
        required_finite_source_number(source_row_0, "x")?,
        required_finite_source_number(source_row_0, "y")?,
        required_finite_source_number(source_row_0, "z")?,
        required_finite_source_number(source_row_1, "x")?,
        required_finite_source_number(source_row_1, "y")?,
        required_finite_source_number(source_row_1, "z")?,
        required_finite_source_number(source_row_2, "x")?,
        required_finite_source_number(source_row_2, "y")?,
        required_finite_source_number(source_row_2, "z")?,
    ])
}

pub(super) fn invalid_source_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
