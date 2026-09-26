use openzt2_game_data::localization::LocalizationDateFormatToken;

pub(super) fn tokenize_blue_fang_localization_date_format(
    authored_date_format: &str,
) -> Vec<LocalizationDateFormatToken> {
    let mut output = Vec::new();
    let mut literal = String::new();
    let mut characters = authored_date_format.chars().peekable();
    let mut quoted = false;
    while let Some(character) = characters.next() {
        match character {
            '\'' => quoted = !quoted,
            _ if quoted => literal.push(character),
            'M' => {
                while characters.next_if_eq(&'M').is_some() {}
                push_localization_date_literal_if_present(&mut output, &mut literal);
                output.push(LocalizationDateFormatToken::MonthAbbreviation);
            }
            'y' => {
                while characters.next_if_eq(&'y').is_some() {}
                push_localization_date_literal_if_present(&mut output, &mut literal);
                output.push(LocalizationDateFormatToken::Year);
            }
            other => literal.push(other),
        }
    }
    push_localization_date_literal_if_present(&mut output, &mut literal);
    output
}

fn push_localization_date_literal_if_present(
    output: &mut Vec<LocalizationDateFormatToken>,
    literal: &mut String,
) {
    if !literal.is_empty() {
        output.push(LocalizationDateFormatToken::Literal(std::mem::take(
            literal,
        )));
    }
}
