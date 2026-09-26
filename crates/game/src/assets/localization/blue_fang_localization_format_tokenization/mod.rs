use std::io;

use openzt2_game_data::localization::{LocalizationFormatArgumentKind, LocalizationFormatToken};

pub(super) fn tokenize_blue_fang_localized_text_format(
    localized_text: &str,
) -> io::Result<Vec<LocalizationFormatToken>> {
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut characters = localized_text.chars().peekable();
    let mut argument_index = 0_u16;
    while let Some(character) = characters.next() {
        if character != '%' {
            literal.push(character);
            continue;
        }
        let Some(mut specifier) = characters.next() else {
            literal.push('%');
            break;
        };
        if specifier == '%' {
            literal.push('%');
            continue;
        }
        if specifier.is_ascii_alphabetic() {
            let mut lookahead = characters.clone();
            while lookahead
                .peek()
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == '_')
            {
                lookahead.next();
            }
            if lookahead.next() == Some('%') {
                characters = lookahead;
                push_literal_if_present(&mut tokens, &mut literal);
                tokens.push(format_argument_token(
                    argument_index,
                    LocalizationFormatArgumentKind::Text,
                )?);
                argument_index = argument_index.checked_add(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "too many format arguments")
                })?;
                continue;
            }
        }
        while matches!(
            specifier,
            '0'..='9' | '.' | '+' | '-' | '#' | ' ' | 'l' | 'h'
        ) {
            let Some(next) = characters.next() else {
                literal.push('%');
                literal.push(specifier);
                push_literal_if_present(&mut tokens, &mut literal);
                return Ok(tokens);
            };
            specifier = next;
        }
        push_literal_if_present(&mut tokens, &mut literal);
        let argument_kind = match specifier {
            'd' | 'i' | 'u' => LocalizationFormatArgumentKind::Integer,
            'f' | 'g' => LocalizationFormatArgumentKind::Decimal,
            _ => LocalizationFormatArgumentKind::Text,
        };
        tokens.push(format_argument_token(argument_index, argument_kind)?);
        argument_index = argument_index.checked_add(1).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "too many format arguments")
        })?;
    }
    push_literal_if_present(&mut tokens, &mut literal);
    Ok(tokens)
}

fn format_argument_token(
    argument_index: u16,
    format_argument_kind: LocalizationFormatArgumentKind,
) -> io::Result<LocalizationFormatToken> {
    Ok(LocalizationFormatToken::Argument {
        format_argument_index: u8::try_from(argument_index).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "localized text contains more than 256 format arguments",
            )
        })?,
        format_argument_kind,
    })
}

fn push_literal_if_present(tokens: &mut Vec<LocalizationFormatToken>, literal: &mut String) {
    if !literal.is_empty() {
        tokens.push(LocalizationFormatToken::Literal(std::mem::take(literal)));
    }
}
