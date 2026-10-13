//! Transient parsed Blue Fang source documents used by live asset lowering.
//!
//! It preserves the repaired, ordered document tree long enough for one typed
//! asset loader to perform semantic lowering, then discards it.

pub mod animation_text_keys;
pub mod authored_behavior_type_identifiers;
pub mod authored_species_purchase_family_queries;
pub mod blue_fang_actor_manifest_model_and_scene_resolution_index;
pub mod blue_fang_source_dependency_reference_discovery;
pub mod blue_fang_source_document_format;
pub mod blue_fang_source_document_parsing;
pub mod blue_fang_source_document_parsing_error;
pub mod blue_fang_source_numeric_lexeme;
pub mod document_semantics;
pub mod ordered_source_document_collection_precedence;
pub mod ordered_source_document_types;
pub mod path;
pub mod resolved_source_record_index;
pub mod source_document_semantic_name;
pub mod ui;
