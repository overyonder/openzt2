use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode,
    ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentSpan},
    path::{authored_source_reference_paths, is_authored_file_reference},
    source_document_semantic_name::{
        canonicalize_source_document_record_key, source_document_names_are_semantically_equal,
    },
};

pub mod resolved_source_record_index_construction;

use resolved_source_record_index_construction::build_precedence_resolved_source_record_index;

#[derive(Clone, Copy)]
struct IndexedElement<'a> {
    document: &'a OrderedSourceDocument,
    element: &'a OrderedSourceDocumentNode,
    root: bool,
}

pub struct SourceIndex<'a> {
    records: BTreeMap<String, IndexedElement<'a>>,
    document_roots_by_source_path: BTreeMap<String, (String, IndexedElement<'a>)>,
}

impl<'a> SourceIndex<'a> {
    pub fn build(
        documents: impl IntoIterator<Item = &'a OrderedSourceDocument>,
    ) -> Result<Self, BindError> {
        Self::build_with_additional_named_record_element_types(documents, &[])
    }

    pub fn build_with_additional_named_record_element_types(
        documents: impl IntoIterator<Item = &'a OrderedSourceDocument>,
        additional_named_record_element_types: &[&str],
    ) -> Result<Self, BindError> {
        let documents = documents.into_iter().collect::<Vec<_>>();
        let document_roots_by_source_path = documents
            .iter()
            .copied()
            .map(|document| {
                let element = &document.root;
                let record_key = element
                    .attribute_named_any(&["id", "typeName", "entityName", "binderType", "key"])
                    .filter(|value| !value.trim().is_empty())
                    .map_or_else(
                        || canonicalize_source_document_record_key(&document.path.key()),
                        canonicalize_source_document_record_key,
                    );
                (
                    document.path.key(),
                    (
                        record_key,
                        IndexedElement {
                            document,
                            element,
                            root: true,
                        },
                    ),
                )
            })
            .collect();
        let index = Self {
            records: build_precedence_resolved_source_record_index(
                documents,
                additional_named_record_element_types,
            )?,
            document_roots_by_source_path,
        };
        index.validate_inheritance()?;
        Ok(index)
    }

    pub fn document_root<'index>(
        &'index self,
        source_path: &str,
    ) -> Option<RecordView<'index, 'a>> {
        let (key, indexed) = self.document_roots_by_source_path.get(source_path)?;
        Some(RecordView {
            index: self,
            key,
            indexed: *indexed,
        })
    }

    pub fn records<'index>(&'index self) -> impl Iterator<Item = RecordView<'index, 'a>> + 'index {
        self.records.iter().map(|(key, indexed)| RecordView {
            index: self,
            key,
            indexed: *indexed,
        })
    }

    pub fn find<'index>(&'index self, key: &str) -> Option<RecordView<'index, 'a>> {
        self.records
            .get_key_value(key)
            .or_else(|| {
                let canonical = canonicalize_source_document_record_key(key);
                self.records.get_key_value(&canonical)
            })
            .map(|(key, indexed)| RecordView {
                index: self,
                key,
                indexed: *indexed,
            })
    }

    fn validate_inheritance(&self) -> Result<(), BindError> {
        let mut validated = BTreeSet::new();
        for start in self.records() {
            let mut current = start;
            let mut visiting = BTreeSet::new();
            loop {
                let element = std::ptr::from_ref(current.indexed.element);
                if validated.contains(&element) {
                    break;
                }
                if !visiting.insert(element) {
                    return Err(BindError::record(
                        &current,
                        format!("source inheritance cycle at {}", current.key),
                    ));
                }
                let Some(base) = current
                    .indexed
                    .element
                    .attribute_named_any(&INHERITANCE_ATTRIBUTES)
                else {
                    break;
                };
                current = current
                    .find_resolved_source_record_by_reference(base)
                    .ok_or_else(|| {
                        BindError::record(&current, format!("unresolved source base {base}"))
                    })?;
            }
            validated.append(&mut visiting);
        }
        Ok(())
    }
}

const INHERITANCE_ATTRIBUTES: [&str; 4] = ["extends", "base", "parentType", "inherit"];

#[derive(Clone, Copy)]
pub struct RecordView<'index, 'document> {
    index: &'index SourceIndex<'document>,
    pub key: &'index str,
    indexed: IndexedElement<'document>,
}

impl<'index, 'document> RecordView<'index, 'document> {
    pub fn source_document_element(self) -> &'document OrderedSourceDocumentNode {
        self.indexed.element
    }

    pub fn descendant_record(&self, name: &str) -> Option<Self> {
        self.descendant_named(name).map(|element| Self {
            index: self.index,
            key: self.key,
            indexed: IndexedElement {
                document: self.indexed.document,
                element,
                root: false,
            },
        })
    }

    pub fn is_document_root_record(self) -> bool {
        self.indexed.root
    }

    pub fn find_resolved_source_record_by_reference(&self, reference: &str) -> Option<Self> {
        if is_authored_file_reference(reference) {
            authored_source_reference_paths(self.indexed.document.path.as_str(), reference)
                .find_map(|path| self.index.document_root(&path.key()))
                .or_else(|| self.index.find(reference))
        } else {
            self.index.find(reference)
        }
    }

    fn base_record(&self) -> Option<Self> {
        self.indexed
            .element
            .attribute_named_any(&INHERITANCE_ATTRIBUTES)
            .and_then(|base| self.find_resolved_source_record_by_reference(base))
    }

    pub fn source_path(&self) -> String {
        self.indexed.document.path.key()
    }

    pub fn find_semantically_equivalent_resolved_source_record_key(
        &self,
        reference: &str,
    ) -> Option<&'index str> {
        self.index
            .records()
            .find(|candidate| {
                source_document_names_are_semantically_equal(candidate.key, reference)
            })
            .map(|candidate| candidate.key)
    }

    pub fn span(&self) -> OrderedSourceDocumentSpan {
        self.indexed.element.span
    }

    pub fn semantic_type(&self) -> String {
        self.value(&["definitionType", "class"])
            .unwrap_or_else(|| self.indexed.element.name.as_str())
            .trim()
            .to_ascii_lowercase()
    }

    /// Tests the resolved authored type hierarchy without retaining that
    /// source-shaped hierarchy in the native catalogue.
    pub fn has_type_token(&self, token: &str) -> bool {
        self.has_type_token_inner(token)
    }

    /// Returns the resolved authored type ancestry as canonical flat tokens.
    /// Live lowering uses this to compile consumer indexes; gameplay never receives
    /// the source inheritance graph.
    pub fn type_tokens(&self) -> Vec<String> {
        fn append(element: &'_ OrderedSourceDocumentNode, output: &mut Vec<String>) {
            for child in element.element_children() {
                output.push(canonicalize_source_document_record_key(child.name.as_str()));
                append(child, output);
            }
        }

        let mut output = self.base_record().map_or_else(Vec::new, |base| {
            let mut ancestors = base.type_tokens();
            ancestors.push(base.key.to_owned());
            ancestors
        });
        if let Some(types) = self.indexed.element.element_children().find(|child| {
            source_document_names_are_semantically_equal(child.name.as_str(), "types")
        }) {
            append(types, &mut output);
        }
        // Preserve authored ancestry order: callers use the final token as the
        // concrete/most-specific kind while the complete vector remains the
        // flattened runtime selector. Sorting here destroyed that distinction.
        let mut seen = BTreeSet::new();
        output.retain(|token| seen.insert(token.clone()));
        output
    }

    fn has_type_token_inner(&self, token: &str) -> bool {
        let direct = self
            .indexed
            .element
            .element_children()
            .find(|child| {
                source_document_names_are_semantically_equal(child.name.as_str(), "types")
            })
            .is_some_and(|types| contains_named_descendant(types, token));
        let inherited = !direct
            && self.base_record().is_some_and(|base| {
                source_document_names_are_semantically_equal(base.key, token)
                    || base.has_type_token_inner(token)
            });
        direct || inherited
    }

    pub fn value(&self, names: &[&str]) -> Option<&'document str> {
        let direct = self.indexed.element.attribute_named_any(names).or_else(|| {
            self.indexed.element.element_children().find_map(|child| {
                names
                    .iter()
                    .any(|wanted| {
                        source_document_names_are_semantically_equal(child.name.as_str(), wanted)
                    })
                    .then(|| {
                        child
                            .attribute_named_any(&["value", "val", "ref", "path", "name"])
                            .or_else(|| child.first_text())
                    })
                    .flatten()
            })
        });
        direct.or_else(|| self.base_record().and_then(|base| base.value(names)))
    }

    pub fn children_named(&self, names: &[&str]) -> Vec<&'document OrderedSourceDocumentNode> {
        let mut output = self
            .indexed
            .element
            .element_children()
            .filter(|child| {
                names.iter().any(|name| {
                    source_document_names_are_semantically_equal(child.name.as_str(), name)
                })
            })
            .collect::<Vec<_>>();
        if output.is_empty() {
            if let Some(base) = self.base_record() {
                output = base.children_named(names);
            }
        }
        output
    }

    pub fn descendant_named(&self, name: &str) -> Option<&'document OrderedSourceDocumentNode> {
        find_named_descendant(self.indexed.element, name).or_else(|| {
            self.base_record()
                .and_then(|base| base.descendant_named(name))
        })
    }

    pub fn descendant_with_attribute(
        &self,
        name: &str,
        attribute: &str,
        value: &str,
    ) -> Option<&'document OrderedSourceDocumentNode> {
        fn find<'a>(
            element: &'a OrderedSourceDocumentNode,
            name: &str,
            attribute: &str,
            value: &str,
        ) -> Option<&'a OrderedSourceDocumentNode> {
            element.element_children().find_map(|child| {
                (source_document_names_are_semantically_equal(child.name.as_str(), name)
                    && child
                        .attribute_named_any(&[attribute])
                        .is_some_and(|candidate| {
                            source_document_names_are_semantically_equal(candidate, value)
                        }))
                .then_some(child)
                .or_else(|| find(child, name, attribute, value))
            })
        }

        find(self.indexed.element, name, attribute, value).or_else(|| {
            self.base_record()
                .and_then(|base| base.descendant_with_attribute(name, attribute, value))
        })
    }
}

fn find_named_descendant<'a>(
    element: &'a OrderedSourceDocumentNode,
    wanted: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), wanted)
            .then_some(child)
            .or_else(|| find_named_descendant(child, wanted))
    })
}

fn contains_named_descendant(element: &'_ OrderedSourceDocumentNode, wanted: &str) -> bool {
    element.element_children().any(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), wanted)
            || contains_named_descendant(child, wanted)
    })
}

#[cfg(test)]
mod tests {
    use crate::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
    };

    use super::SourceIndex;

    fn parse(path: &str, source: &str) -> OrderedSourceDocument {
        parse_blue_fang_source_document(AssetPath::new(path), source.as_bytes())
            .expect("valid test document")
    }

    #[test]
    fn type_tokens_are_scoped_to_the_authored_types_tree() {
        let documents = [parse(
            "entities/objects/eggs/ai/test_egg.xml",
            r#"
                <BFTypedBinder binderType="test_egg">
                    <types><entity><egg><test_egg/></egg></entity></types>
                    <shared><BFAINoPerceive><entrance/></BFAINoPerceive></shared>
                </BFTypedBinder>
            "#,
        )];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let egg = index.find("test_egg").expect("indexed egg");

        assert!(egg.has_type_token("egg"));
        assert!(!egg.has_type_token("entrance"));
    }

    #[test]
    fn type_tokens_follow_authored_record_inheritance() {
        let documents = [
            parse(
                "entities/objects/buildings/ai/entrance.xml",
                r#"
                    <BFTypedBinder binderType="entrance" abstract="true">
                        <types><entity><building><entrance/></building></entity></types>
                    </BFTypedBinder>
                "#,
            ),
            parse(
                "entities/objects/buildings/ai/frontgate.xml",
                r#"<BFTypedBinder binderType="frontgate" extends="entrance"/>"#,
            ),
        ];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let gate = index.find("frontgate").expect("indexed gate");

        assert!(gate.has_type_token("entrance"));
    }

    #[test]
    fn inherits_from_a_binder_in_a_different_directory() {
        let documents = [
            parse(
                "registered/parents/other_file.xml",
                r#"<BFTypedBinder binderType="mod_parent" scale="2" extends="animal"/>"#,
            ),
            parse(
                "registered/children/new_file.xml",
                r#"<BFTypedBinder binderType="mod_child" extends="..\parents\other_file.xml"/>"#,
            ),
            parse(
                "stock/animal.xml",
                r#"<BFTypedBinder binderType="animal"><types><animal/></types></BFTypedBinder>"#,
            ),
        ];
        let index = SourceIndex::build(&documents).expect("valid inheritance");
        let child = index.find("mod_child").expect("indexed child");

        assert!(child.has_type_token("animal"));
        assert_eq!(child.value(&["scale"]), Some("2"));
    }

    #[test]
    fn authored_components_remain_structural_descendants() {
        let documents = [parse(
            "world/cameras/overheadcam.xml",
            r#"
                <BFPhysObj>
                    <BFOverheadCameraComponent pitchRotate="1.25"/>
                </BFPhysObj>
            "#,
        )];
        let index = SourceIndex::build(&documents).expect("valid source index");
        let camera = index
            .find("world/cameras/overheadcam.xml")
            .expect("indexed camera");

        assert!(!camera.has_type_token("BFOverheadCameraComponent"));
        assert!(
            camera
                .descendant_named("BFOverheadCameraComponent")
                .is_some()
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindError {
    pub virtual_path: String,
    pub span: OrderedSourceDocumentSpan,
    pub message: String,
}

impl BindError {
    pub fn at(
        document: &OrderedSourceDocument,
        element: &'_ OrderedSourceDocumentNode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            virtual_path: document.path.key(),
            span: element.span,
            message: message.into(),
        }
    }

    pub fn record(record: &RecordView<'_, '_>, message: impl Into<String>) -> Self {
        Self {
            virtual_path: record.source_path(),
            span: record.span(),
            message: message.into(),
        }
    }
}

impl fmt::Display for BindError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}..{}: {}",
            self.virtual_path, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for BindError {}
