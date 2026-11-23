#[derive(Clone, Debug)]
pub(super) enum LocalizationDateTimeToken {
    Literal(String),
    Month(usize),
    Day(usize),
    Year(usize),
    Era,
    Hour12(usize),
    Hour24(usize),
    Minute(usize),
    Second(usize),
    Period(usize),
}

#[repr(usize)]
#[derive(Clone, Copy, Debug)]
pub enum LocalizationDateTimeFormat {
    MonthYear,
    DayDate,
    MonthDayYear,
    MinutesSeconds,
}

pub(super) fn tokenize_blue_fang_localization_date_format(
    format: &str,
    is_time: bool,
) -> Vec<LocalizationDateTimeToken> {
    use LocalizationDateTimeToken as Token;
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut characters = format.chars().peekable();
    let mut quoted = false;
    while let Some(character) = characters.next() {
        if character == '\'' {
            if characters.next_if_eq(&'\'').is_some() {
                literal.push('\'');
            } else {
                quoted = !quoted;
            }
            continue;
        }
        let field = match (quoted, is_time, character) {
            (false, false, 'M') => Token::Month,
            (false, false, 'd') => Token::Day,
            (false, false, 'y') => Token::Year,
            (false, false, 'g') => |_| Token::Era,
            (false, true, 'h') => Token::Hour12,
            (false, true, 'H') => Token::Hour24,
            (false, true, 'm') => Token::Minute,
            (false, true, 's') => Token::Second,
            (false, true, 't') => Token::Period,
            _ => {
                literal.push(character);
                continue;
            }
        } as fn(usize) -> Token;
        let mut width = 1;
        while characters.next_if_eq(&character).is_some() {
            width += 1;
        }
        if !literal.is_empty() {
            tokens.push(Token::Literal(std::mem::take(&mut literal)));
        }
        tokens.push(field(width));
    }
    if !literal.is_empty() {
        tokens.push(Token::Literal(literal));
    }
    tokens
}
