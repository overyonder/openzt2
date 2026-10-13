//! NIF document framing, parsed-block construction, and allocation-free block lookup.

use super::{
    super::native_source_byte_reading::{
        read_i32_little_endian, read_u16_little_endian, read_u32_little_endian,
    },
    block_payload_parsing::parse_block_payload,
    document_source_types::{
        NetImmerseNifBlock, NetImmerseNifDocument, NetImmerseNifFooter, NetImmerseNifHeader,
    },
    source_error::NetImmerseNifSourceError,
};

const NETIMMERSE_HEADER_PREFIX: &[u8] = b"NetImmerse File Format, Version ";

type Result<T> = std::result::Result<T, NetImmerseNifSourceError>;

impl NetImmerseNifDocument {
    pub(in super::super) fn parse(source_path: String, source_bytes: &[u8]) -> Result<Self> {
        let header_newline = source_bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| {
                NetImmerseNifSourceError::invalid_data(
                    &source_path,
                    "missing NetImmerse header newline",
                )
            })?;
        let version_text_start = NETIMMERSE_HEADER_PREFIX.len();
        if header_newline < version_text_start {
            return Err(NetImmerseNifSourceError::invalid_data(
                &source_path,
                "truncated NetImmerse header",
            ));
        }
        let version_text = std::str::from_utf8(&source_bytes[version_text_start..header_newline])
            .map_err(|error| {
                NetImmerseNifSourceError::invalid_utf8(&source_path, error.to_string())
            })?
            .trim_end_matches('\r')
            .to_owned();
        let mut cursor = header_newline + 1;
        let encoded_version = read_u32_little_endian(
            source_bytes,
            &mut cursor,
            &source_path,
            "encoded NIF version",
        )?;
        let declared_block_count =
            read_u32_little_endian(source_bytes, &mut cursor, &source_path, "block count")?;
        let block_type_count =
            read_u16_little_endian(source_bytes, &mut cursor, &source_path, "block type count")?;
        let block_type_names = (0..block_type_count)
            .map(|_| {
                let byte_length = read_u32_little_endian(
                    source_bytes,
                    &mut cursor,
                    &source_path,
                    "block type name length",
                )?;
                let byte_length = usize::try_from(byte_length).map_err(|_| {
                    NetImmerseNifSourceError::invalid_data(
                        &source_path,
                        "block type name length does not fit usize",
                    )
                })?;
                let end = cursor.checked_add(byte_length).ok_or_else(|| {
                    NetImmerseNifSourceError::invalid_data(
                        &source_path,
                        "block type name length overflows file cursor",
                    )
                })?;
                let name_source_bytes = source_bytes.get(cursor..end).ok_or_else(|| {
                    NetImmerseNifSourceError::invalid_data(
                        &source_path,
                        "truncated block type name",
                    )
                })?;
                let block_type_name = std::str::from_utf8(name_source_bytes)
                    .map_err(|error| {
                        NetImmerseNifSourceError::invalid_utf8(&source_path, error.to_string())
                    })?
                    .to_owned();
                cursor = end;
                Ok(block_type_name)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut block_type_indices = Vec::with_capacity(declared_block_count as usize);
        for _ in 0..declared_block_count {
            let block_type_index = read_u16_little_endian(
                source_bytes,
                &mut cursor,
                &source_path,
                "block type index",
            )?;
            if usize::from(block_type_index) >= block_type_names.len() {
                return Err(NetImmerseNifSourceError::invalid_data(
                    &source_path,
                    format!("block type index {block_type_index} is outside block type table"),
                ));
            }
            block_type_indices.push(block_type_index);
        }
        let group_count =
            read_u32_little_endian(source_bytes, &mut cursor, &source_path, "group count")?;
        let groups = (0..group_count)
            .map(|_| {
                read_u32_little_endian(source_bytes, &mut cursor, &source_path, "group record")
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        let block_payload_offset_bytes = u32::try_from(cursor).map_err(|_| {
            NetImmerseNifSourceError::invalid_data(
                &source_path,
                "block payload offset does not fit u32",
            )
        })?;
        let mut block_cursor = cursor;
        let parsed_blocks = block_type_indices
            .iter()
            .enumerate()
            .map(|(block_index, block_type_index)| {
                let block_type_name = &block_type_names[usize::from(*block_type_index)];
                let source_byte_offset = u32::try_from(block_cursor).map_err(|_| {
                    NetImmerseNifSourceError::invalid_data(
                        &source_path,
                        "block byte offset does not fit u32",
                    )
                })?;
                let block_prelude = Some(read_u32_little_endian(
                    source_bytes,
                    &mut block_cursor,
                    &source_path,
                    "block prelude",
                )?);
                let payload = parse_block_payload(
                    source_bytes,
                    &mut block_cursor,
                    &source_path,
                    encoded_version,
                    block_type_name,
                )
                .map_err(|error| error.with_block_context(block_index as u32, block_type_name))?;
                let source_byte_end = u32::try_from(block_cursor).map_err(|_| {
                    NetImmerseNifSourceError::invalid_data(
                        &source_path,
                        "block byte end does not fit u32",
                    )
                })?;
                Ok(NetImmerseNifBlock {
                    index: block_index as u32,
                    block_type_name: block_type_name.clone(),
                    source_byte_offset,
                    source_byte_size: source_byte_end.saturating_sub(source_byte_offset),
                    block_prelude,
                    payload,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let block_payload_size_bytes =
            u32::try_from(block_cursor.saturating_sub(cursor)).map_err(|_| {
                NetImmerseNifSourceError::invalid_data(
                    &source_path,
                    "block payload size does not fit u32",
                )
            })?;
        let root_count =
            read_u32_little_endian(source_bytes, &mut block_cursor, &source_path, "root count")?;
        let root_block_references = (0..root_count)
            .map(|_| {
                read_i32_little_endian(source_bytes, &mut block_cursor, &source_path, "root ref")
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            source_path,
            header: NetImmerseNifHeader {
                version_text,
                encoded_version,
                declared_block_count,
                block_type_names,
                block_type_indices,
                groups,
            },
            footer: NetImmerseNifFooter {
                root_block_references,
            },
            parsed_blocks,
            block_payload_offset_bytes,
            block_payload_size_bytes,
        })
    }

    /// Resolves a signed NIF block reference without allocating or copying payload data.
    pub(in super::super) fn block(&self, block_reference: i32) -> Option<&NetImmerseNifBlock> {
        usize::try_from(block_reference)
            .ok()
            .and_then(|block_index| self.parsed_blocks.get(block_index))
            .filter(|block| block.index as i32 == block_reference)
    }

    pub(in super::super) fn blocks(
        &self,
    ) -> impl ExactSizeIterator<Item = &NetImmerseNifBlock> + DoubleEndedIterator {
        self.parsed_blocks.iter()
    }
}
