use std::str::FromStr;

/// Parses an authored Blue Fang number after applying only numeric spellings
/// observed in shipped source. Apostrophes between digits are separators. A
/// single comma between digits is a decimal separator when no decimal point is
/// present.
pub(in crate::assets) fn parse_blue_fang_source_numeric_lexeme<T: FromStr>(
    source_value: &str,
) -> Option<T> {
    let source_value = source_value
        .trim()
        .strip_suffix(['f', 'F'])
        .unwrap_or_else(|| source_value.trim());
    let source_bytes = source_value.as_bytes();
    let comma_count = source_bytes
        .iter()
        .filter(|source_byte| **source_byte == b',')
        .count();
    let comma_is_observed_decimal_separator = comma_count == 1
        && !source_bytes.contains(&b'.')
        && source_bytes
            .iter()
            .position(|source_byte| *source_byte == b',')
            .is_some_and(|comma_index| {
                comma_index != 0
                    && comma_index + 1 != source_bytes.len()
                    && source_bytes[comma_index - 1].is_ascii_digit()
                    && source_bytes[comma_index + 1].is_ascii_digit()
            });
    if comma_count != 0 && !comma_is_observed_decimal_separator {
        return None;
    }
    if source_bytes
        .iter()
        .enumerate()
        .any(|(source_index, source_byte)| {
            *source_byte == b'\''
                && (source_index == 0
                    || source_index + 1 == source_bytes.len()
                    || !source_bytes[source_index - 1].is_ascii_digit()
                    || !source_bytes[source_index + 1].is_ascii_digit())
        })
    {
        return None;
    }
    if !source_bytes.contains(&b'\'') && !comma_is_observed_decimal_separator {
        return source_value.parse().ok();
    }

    source_value
        .chars()
        .filter_map(|source_character| match source_character {
            '\'' => None,
            ',' => Some('.'),
            source_character => Some(source_character),
        })
        .collect::<String>()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::parse_blue_fang_source_numeric_lexeme;

    #[test]
    fn parses_shipped_digit_separator_and_decimal_comma_spellings() {
        assert_eq!(
            parse_blue_fang_source_numeric_lexeme::<f64>("1.0000'0"),
            Some(1.0)
        );
        assert_eq!(
            parse_blue_fang_source_numeric_lexeme::<f64>("0,1"),
            Some(0.1)
        );
    }
}
