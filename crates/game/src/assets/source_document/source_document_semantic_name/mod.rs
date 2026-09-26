/// Allocation-free comparison for the case/punctuation-insensitive names used
/// throughout Blue Fang source documents.
pub(crate) fn source_document_names_are_semantically_equal(left: &str, right: &str) -> bool {
    left.bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase())
        .eq(right
            .bytes()
            .filter(u8::is_ascii_alphanumeric)
            .map(|byte| byte.to_ascii_lowercase()))
}

pub(crate) fn canonicalize_source_document_record_key(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}
