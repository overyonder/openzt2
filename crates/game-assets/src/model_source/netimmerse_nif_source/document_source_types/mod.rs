//! NetImmerse NIF document header, footer, block metadata, and parsed block records.

use super::block_payload::NetImmerseNifBlockPayload;

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNifDocument {
    pub(in super::super) source_path: String,
    pub(in super::super) header: NetImmerseNifHeader,
    pub(in super::super) footer: NetImmerseNifFooter,
    pub(super) parsed_blocks: Vec<NetImmerseNifBlock>,
    pub(in super::super) block_payload_offset_bytes: u32,
    pub(in super::super) block_payload_size_bytes: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNifHeader {
    pub(in super::super) version_text: String,
    pub(in super::super) encoded_version: u32,
    pub(in super::super) declared_block_count: u32,
    pub(in super::super) block_type_names: Vec<String>,
    pub(in super::super) block_type_indices: Vec<u16>,
    pub(in super::super) groups: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) struct NetImmerseNifFooter {
    pub(in super::super) root_block_references: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in super::super) struct NetImmerseNifBlock {
    pub(in super::super) index: u32,
    pub(in super::super) block_type_name: String,
    pub(in super::super) source_byte_offset: u32,
    pub(in super::super) source_byte_size: u32,
    pub(in super::super) block_prelude: Option<u32>,
    pub(in super::super) payload: NetImmerseNifBlockPayload,
}
