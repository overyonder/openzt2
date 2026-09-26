//! Transient parsed Blue Fang source documents used by live asset lowering.
//!
//! It preserves the repaired, ordered document tree long enough for one typed
//! asset loader to perform semantic lowering, then discards it.

pub(in crate::assets) mod animation_text_keys;
pub(in crate::assets) mod blue_fang_actor_manifest_model_and_scene_resolution_index;
pub(in crate::assets) mod blue_fang_source_dependency_reference_discovery;
pub(crate) mod blue_fang_source_document_format;
pub(crate) mod blue_fang_source_document_parsing;
pub(crate) mod blue_fang_source_document_parsing_error;
pub(in crate::assets) mod blue_fang_source_numeric_lexeme;
pub(crate) mod document_semantics;
pub(in crate::assets) mod ordered_source_document_collection_precedence;
pub(crate) mod ordered_source_document_types;
pub(crate) mod path;
pub(crate) mod resolved_source_record_index;
pub(crate) mod source_document_semantic_name;
pub(crate) mod ui;
