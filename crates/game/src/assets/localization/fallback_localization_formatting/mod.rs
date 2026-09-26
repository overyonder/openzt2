pub(super) fn write_fallback_currency_amount(
    currency_amount_cents: i64,
    include_fractional_currency_cents: bool,
    target: &mut impl std::fmt::Write,
) -> std::fmt::Result {
    openzt2_game_data::localization::LocalizationCatalog::write_currency_amount_with_symbol_and_comma_grouped_whole_dollars(
        "$",
        currency_amount_cents,
        include_fractional_currency_cents,
        target,
    )
}
