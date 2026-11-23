//! Portable Windows locale defaults for currency and Gregorian field names.

use std::{fmt, sync::OnceLock};

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct WindowsLocaleConventions {
    pub(super) currency: CurrencyConventions,
    pub(super) calendar: CalendarConventions,
}

#[derive(Deserialize)]
pub(super) struct CurrencyConventions {
    pub(super) symbol: String,
    decimal_separator: String,
    grouping_separator: String,
    repeating_group_width: u32,
    fractional_digits: u32,
    positive_order: usize,
    negative_order: usize,
    negative_sign: String,
}

#[derive(Deserialize)]
pub(super) struct CalendarConventions {
    pub(super) month_names: [String; 12],
    pub(super) abbreviated_month_names: [String; 12],
    pub(super) weekday_names: [String; 7],
    pub(super) abbreviated_weekday_names: [String; 7],
    pub(super) am_designator: String,
    pub(super) pm_designator: String,
    pub(super) era: String,
}

#[derive(Deserialize)]
struct LocaleTable(Vec<WindowsLocaleConventions>, Vec<(u32, usize)>);

pub(super) fn windows_locale_conventions(locale: &str) -> &'static WindowsLocaleConventions {
    static LOCALES: OnceLock<LocaleTable> = OnceLock::new();
    let locales = LOCALES.get_or_init(|| {
        serde_json::from_str(include_str!("locale_conventions.json"))
            .expect("bundled Windows locale conventions must be valid")
    });
    let find = |id| {
        let row = locales.1.binary_search_by_key(&id, |(id, _)| *id).ok()?;
        locales.0.get(locales.1[row].1)
    };
    locale
        .parse::<u32>()
        .ok()
        .and_then(find)
        .or_else(|| find(1033))
        .expect("bundled Windows locale conventions contain US English")
}

impl CurrencyConventions {
    pub(super) fn write_amount(
        &self,
        symbol: &str,
        cents: i64,
        include_cents: bool,
        target: &mut impl fmt::Write,
    ) -> fmt::Result {
        // Windows positive and negative currency orders.
        const POSITIVE: [&str; 4] = ["$#", "#$", "$ #", "# $"];
        const NEGATIVE: [&str; 17] = [
            "($#)", "-$#", "$-#", "$#-", "(#$)", "-#$", "#-$", "#$-", "-# $", "-$ #", "# $-",
            "$ #-", "$ -#", "#- $", "($ #)", "(# $)", "$- #",
        ];
        let pattern = if cents < 0 {
            NEGATIVE[self.negative_order]
        } else {
            POSITIVE[self.positive_order]
        };

        let digits = if include_cents {
            self.fractional_digits
        } else {
            0
        };
        let absolute_cents = u128::from(cents.unsigned_abs());
        let scale = 10_u128.pow(digits);
        let rounding = if include_cents { 50 } else { 0 };
        let amount = (absolute_cents * scale + rounding) / 100;
        for token in pattern.chars() {
            match token {
                '$' => target.write_str(symbol)?,
                '-' => target.write_str(&self.negative_sign)?,
                '#' => {
                    self.write_grouped_whole_amount(amount / scale, target)?;
                    if digits != 0 {
                        target.write_str(&self.decimal_separator)?;
                        write!(
                            target,
                            "{:0width$}",
                            amount % scale,
                            width = digits as usize
                        )?;
                    }
                }
                other => target.write_char(other)?,
            }
        }
        Ok(())
    }

    fn write_grouped_whole_amount(
        &self,
        amount: u128,
        target: &mut impl fmt::Write,
    ) -> fmt::Result {
        if amount < 1_000 {
            return write!(target, "{amount}");
        }
        let group_base = 10_u128.pow(self.repeating_group_width);
        let upper = amount / 1_000;
        let mut divisor = 1;
        while upper / divisor >= group_base {
            divisor *= group_base;
        }
        write!(target, "{}", upper / divisor)?;
        while divisor > 1 {
            divisor /= group_base;
            target.write_str(&self.grouping_separator)?;
            write!(
                target,
                "{:0width$}",
                upper / divisor % group_base,
                width = self.repeating_group_width as usize,
            )?;
        }
        target.write_str(&self.grouping_separator)?;
        write!(target, "{:03}", amount % 1_000)
    }
}
