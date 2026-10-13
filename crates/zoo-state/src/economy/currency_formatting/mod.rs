pub fn write_whole_dollar_currency_with_thousands_separators(output: &mut String, cents: i64) {
    // Reports truncate cents before choosing the sign, so -99 cents displays as $0.
    let whole_dollar_cents = cents / 100 * 100;
    let _ = openzt2_game_data::localization::LocalizationCatalog::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
        "$",
        whole_dollar_cents,
        false,
        output,
    );
}
