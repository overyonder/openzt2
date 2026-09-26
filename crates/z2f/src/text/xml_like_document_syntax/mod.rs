use std::path::Path;

/// Repairs the observed malformed XML-like source shapes for one asset path.
#[must_use]
pub fn repair_observed_blue_fang_xml_like_document_syntax(path: &Path, text: &str) -> String {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let mut repaired = normalize_blue_fang_xml_like_markup(text);
    if extension.eq_ignore_ascii_case("bfm") && repaired.starts_with("<?xml") {
        repaired = repaired
            .find("?>")
            .map(|end| repaired[end + 2..].to_owned())
            .unwrap_or(repaired);
    }
    repaired = if extension.eq_ignore_ascii_case("psys") {
        format!("<particleSystems>{repaired}</particleSystems>")
    } else if extension.eq_ignore_ascii_case("bfm") {
        format!("<actor>{repaired}</actor>")
    } else {
        repaired
    };
    if roxmltree::Document::parse(&repaired).is_err() {
        merge_repeated_top_level_document_roots(repaired)
    } else {
        repaired
    }
}

// Narrow syntax repair for malformed Blue Fang XML-like source documents.

use std::collections::BTreeSet;

const BFXML_PREFIX_NAMESPACE_BASE: &str = "urn:openzt2:bfxml-prefix:";

/// Repairs Blue Fang's XML-like syntax without interpreting its object model:
/// undeclared element prefixes, raw ampersands/less-than signs in attributes,
/// duplicate attributes, doubled assignment signs, repeated top-level roots,
/// and non-markup bytes trailing the final tag.
fn normalize_blue_fang_xml_like_markup(text: &str) -> String {
    let prefixes = collect_prefixes(text);
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < text.len() {
        let remainder = &text[cursor..];
        if remainder.starts_with("<!--") {
            cursor = copy_through(text, cursor, "-->", &mut output);
        } else if remainder.starts_with("<![CDATA[") {
            cursor = copy_through(text, cursor, "]]>", &mut output);
        } else if remainder.starts_with("<?") {
            cursor = copy_through(text, cursor, "?>", &mut output);
        } else if remainder.starts_with("<!") {
            cursor = copy_through(text, cursor, ">", &mut output);
        } else if remainder.starts_with('<') {
            cursor = normalize_tag(text, cursor, &mut output);
        } else if remainder.starts_with('&') {
            cursor = normalize_ampersand(text, cursor, &mut output);
        } else if let Some(character) = remainder.chars().next() {
            output.push(character);
            cursor += character.len_utf8();
        } else {
            break;
        }
    }
    trim_trailing_junk(inject_prefix_declarations(output, &prefixes))
}

fn normalize_tag(text: &str, start: usize, output: &mut String) -> usize {
    if text[start..].starts_with("</") {
        return copy_through(text, start, ">", output);
    }
    let Some(end) = find_tag_end(text, start) else {
        output.push_str(&text[start..]);
        return text.len();
    };
    let tag = &text[start + 1..end - 1];
    let self_closing = tag.trim_end().ends_with('/');
    let tag = tag.trim_end_matches(|character: char| character.is_whitespace() || character == '/');
    let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
    output.push('<');
    output.push_str(&tag[..name_end]);

    let mut attributes = &tag[name_end..];
    let mut normalized_attributes = Vec::new();
    while let Some((name, value, remainder)) = take_attribute(attributes) {
        let name = name.strip_suffix("__duplicate").unwrap_or(name);
        if let Some(attribute) = normalized_attributes
            .iter_mut()
            .find(|(existing, _)| *existing == name)
        {
            *attribute = (name, value);
        } else {
            normalized_attributes.push((name, value));
        }
        if remainder.len() == attributes.len() {
            break;
        }
        attributes = remainder;
    }
    for (name, value) in normalized_attributes {
        output.push(' ');
        output.push_str(name);
        if let Some(value) = value {
            output.push_str("=\"");
            escape_attribute(value, output);
            output.push('"');
        }
    }
    if self_closing {
        output.push('/');
    }
    output.push('>');
    end
}

fn take_attribute(input: &str) -> Option<(&str, Option<&str>, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    let name_end = input
        .find(|character: char| character.is_whitespace() || character == '=')
        .unwrap_or(input.len());
    if name_end == 0 {
        return None;
    }
    let name = &input[..name_end];
    let mut remainder = input[name_end..].trim_start();
    if !remainder.starts_with('=') {
        return Some((name, None, remainder));
    }
    remainder = remainder[1..].trim_start();
    if remainder.starts_with('=') {
        remainder = remainder[1..].trim_start();
    }
    let Some(first) = remainder.chars().next() else {
        return Some((name, Some(""), remainder));
    };
    if first == '"' || first == '\'' {
        let body = &remainder[first.len_utf8()..];
        let end = body.find(first).unwrap_or(body.len());
        let tail = body.get(end + first.len_utf8()..).unwrap_or_default();
        Some((name, Some(&body[..end]), tail))
    } else {
        let end = remainder
            .find(char::is_whitespace)
            .unwrap_or(remainder.len());
        Some((name, Some(&remainder[..end]), &remainder[end..]))
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    let mut cursor = 0;
    while cursor < value.len() {
        let remainder = &value[cursor..];
        if remainder.starts_with('&') {
            cursor = normalize_ampersand(value, cursor, output);
        } else if remainder.starts_with('<') {
            output.push_str("&lt;");
            cursor += 1;
        } else if remainder.starts_with('"') {
            output.push_str("&quot;");
            cursor += 1;
        } else if let Some(character) = remainder.chars().next() {
            output.push(character);
            cursor += character.len_utf8();
        }
    }
}

fn normalize_ampersand(text: &str, start: usize, output: &mut String) -> usize {
    let body_start = start + 1;
    let terminator = text[body_start..]
        .find(';')
        .map(|relative| body_start + relative);
    if let Some(end) = terminator {
        let body = &text[body_start..end];
        if is_xml_entity(body) {
            output.push_str(&text[start..=end]);
            return end + 1;
        }
    }
    output.push_str("&amp;");
    body_start
}

fn is_xml_entity(body: &str) -> bool {
    matches!(body, "amp" | "lt" | "gt" | "quot" | "apos")
        || body
            .strip_prefix("#x")
            .or_else(|| body.strip_prefix("#X"))
            .is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        || body.strip_prefix('#').is_some_and(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn collect_prefixes(text: &str) -> BTreeSet<String> {
    let mut prefixes = BTreeSet::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find('<') {
        cursor += relative;
        let remainder = &text[cursor..];
        if remainder.starts_with("<!--") {
            cursor = skip_through(text, cursor, "-->");
            continue;
        }
        if remainder.starts_with("<![CDATA[") {
            cursor = skip_through(text, cursor, "]]>");
            continue;
        }
        if remainder.starts_with("<?") {
            cursor = skip_through(text, cursor, "?>");
            continue;
        }
        if remainder.starts_with("<!") {
            cursor = skip_through(text, cursor, ">");
            continue;
        }
        let closing = remainder.starts_with("</");
        cursor += if closing { 2 } else { 1 };
        let end = text[cursor..]
            .find(|character: char| character.is_whitespace() || matches!(character, '/' | '>'))
            .map_or(text.len(), |relative| cursor + relative);
        collect_name_prefix(&text[cursor..end], &mut prefixes);
        if !closing {
            let tag_end = find_tag_end(text, cursor.saturating_sub(1)).unwrap_or(text.len());
            let mut attributes = &text[end..tag_end.saturating_sub(1)];
            while let Some((name, _, remainder)) = take_attribute(attributes) {
                if !name.starts_with("xmlns:") {
                    collect_name_prefix(name, &mut prefixes);
                }
                if remainder.len() == attributes.len() {
                    break;
                }
                attributes = remainder;
            }
            cursor = tag_end;
            continue;
        }
        cursor = end;
    }
    prefixes
}

fn collect_name_prefix(name: &str, prefixes: &mut BTreeSet<String>) {
    if let Some((prefix, _)) = name.split_once(':') {
        if !prefix.is_empty() && prefix != "xml" && prefix != "xmlns" {
            prefixes.insert(prefix.to_owned());
        }
    }
}

fn inject_prefix_declarations(mut text: String, prefixes: &BTreeSet<String>) -> String {
    let Some((start, name_end)) = first_start_tag_name_range(&text) else {
        return text;
    };
    let root_end = text[start..]
        .find('>')
        .map_or(name_end, |offset| start + offset);
    let root = &text[start..root_end];
    let declarations = prefixes.iter().fold(String::new(), |mut output, prefix| {
        if !root.contains(&format!("xmlns:{prefix}=")) {
            output.push_str(" xmlns:");
            output.push_str(prefix);
            output.push_str("=\"");
            output.push_str(BFXML_PREFIX_NAMESPACE_BASE);
            output.push_str(prefix);
            output.push('"');
        }
        output
    });
    text.insert_str(name_end, &declarations);
    text
}

fn merge_repeated_top_level_document_roots(mut text: String) -> String {
    let Some((root_start, root_name_end)) = first_start_tag_name_range(&text) else {
        return text;
    };
    let root_name = text[root_start + 1..root_name_end].to_owned();
    let open = format!("<{root_name}");
    while let Some((close_start, close_end)) = top_level_root_close(&text, root_start) {
        let next = skip_spacing(&text, close_end);
        if text[next..].starts_with(&open)
            && text[next + open.len()..]
                .chars()
                .next()
                .is_some_and(|character| {
                    character.is_whitespace() || matches!(character, '/' | '>')
                })
        {
            if let Some(open_end) = find_tag_end(&text, next) {
                text.replace_range(close_start..open_end, "");
                continue;
            }
        }
        break;
    }
    text
}

/// Finds the close tag paired with the actual document root. Unlike a textual
/// `find("</Root>")`, this cannot mistake a nested equal-named widget for a
/// repeated top-level source root.
fn top_level_root_close(text: &str, root_start: usize) -> Option<(usize, usize)> {
    let mut cursor = root_start;
    let mut depth = 0_u32;
    while let Some(relative) = text[cursor..].find('<') {
        let start = cursor + relative;
        let remainder = &text[start..];
        if remainder.starts_with("<!--") {
            cursor = skip_through(text, start, "-->");
            continue;
        }
        if remainder.starts_with("<![CDATA[") {
            cursor = skip_through(text, start, "]]>");
            continue;
        }
        if remainder.starts_with("<?") {
            cursor = skip_through(text, start, "?>");
            continue;
        }
        if remainder.starts_with("<!") {
            cursor = skip_through(text, start, ">");
            continue;
        }
        let end = find_tag_end(text, start)?;
        let tag = text[start + 1..end - 1].trim();
        if tag.starts_with('/') {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some((start, end));
            }
        } else if !tag.ends_with('/') {
            depth = depth.checked_add(1)?;
        }
        cursor = end;
    }
    None
}

fn first_start_tag_name_range(text: &str) -> Option<(usize, usize)> {
    let mut cursor = 0;
    loop {
        let start = cursor + text[cursor..].find('<')?;
        let remainder = &text[start..];
        let marker = if remainder.starts_with("<!--") {
            Some("-->")
        } else if remainder.starts_with("<?") {
            Some("?>")
        } else if remainder.starts_with("<!") || remainder.starts_with("</") {
            Some(">")
        } else {
            None
        };
        if let Some(marker) = marker {
            cursor = skip_through(text, start, marker);
            continue;
        }
        let name_end = text[start + 1..]
            .find(|character: char| character.is_whitespace() || matches!(character, '/' | '>'))
            .map_or(text.len(), |offset| start + 1 + offset);
        return Some((start, name_end));
    }
}

fn trim_trailing_junk(mut text: String) -> String {
    if let Some(end) = text.rfind('>') {
        if !text[end + 1..].trim().is_empty() {
            text.truncate(end + 1);
        }
    }
    text
}

fn find_tag_end(text: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, character) in text[start..].char_indices() {
        match (quote, character) {
            (Some(active), value) if value == active => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '>') => return Some(start + offset + 1),
            _ => {}
        }
    }
    None
}

fn skip_spacing(text: &str, mut cursor: usize) -> usize {
    while let Some(character) = text.get(cursor..).and_then(|rest| rest.chars().next()) {
        if character.is_whitespace() || character == '\u{feff}' {
            cursor += character.len_utf8();
        } else {
            break;
        }
    }
    cursor
}

fn copy_through(text: &str, start: usize, marker: &str, output: &mut String) -> usize {
    let end = skip_through(text, start, marker);
    output.push_str(&text[start..end]);
    end
}

fn skip_through(text: &str, start: usize, marker: &str) -> usize {
    text[start..]
        .find(marker)
        .map_or(text.len(), |offset| start + offset + marker.len())
}
