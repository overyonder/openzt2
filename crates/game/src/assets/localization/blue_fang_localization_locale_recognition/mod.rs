pub(super) fn source_path_contains_english_localization(path: &str) -> bool {
    locale_identifier_from_blue_fang_source_path(path).as_deref() == Some("en-US")
}

pub(super) fn locale_identifier_from_blue_fang_source_path(path: &str) -> Option<String> {
    let path_components = path.split(['/', '\\']).collect::<Vec<_>>();
    let language_directory_index = path_components
        .iter()
        .position(|component| component.eq_ignore_ascii_case("lang"))?;
    normalize_blue_fang_locale_identifier(path_components.get(language_directory_index + 1)?)
}

fn normalize_blue_fang_locale_identifier(value: &str) -> Option<String> {
    let locale_identifier = match value.to_ascii_lowercase().as_str() {
        "oem1" | "1033" => "en-US",
        "1028" => "zh-TW",
        "1029" => "cs-CZ",
        "1030" => "da-DK",
        "1031" => "de-DE",
        "1034" | "3082" => "es-ES",
        "1035" => "fi-FI",
        "1036" => "fr-FR",
        "1038" => "hu-HU",
        "1040" => "it-IT",
        "1041" => "ja-JP",
        "1042" => "ko-KR",
        "1043" => "nl-NL",
        "1044" => "nb-NO",
        "1045" => "pl-PL",
        "1046" => "pt-BR",
        "1049" => "ru-RU",
        "1053" => "sv-SE",
        "2052" => "zh-CN",
        "2057" => "en-GB",
        "2070" => "pt-PT",
        other if other.len() == 2 => return Some(other.to_owned()),
        _ => return None,
    };
    Some(locale_identifier.to_owned())
}
