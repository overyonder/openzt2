//! Portable Windows locale defaults for currency and Gregorian field names.

use std::{fmt, sync::OnceLock};

use serde::Deserialize;

#[cfg(windows)]
mod host_locale_conventions;

#[derive(Deserialize, PartialEq, Eq)]
pub(super) struct WindowsLocaleConventions {
    pub(super) currency: CurrencyConventions,
    pub(super) calendar: CalendarConventions,
}

#[derive(Deserialize, PartialEq, Eq)]
pub(super) struct CurrencyConventions {
    pub(super) symbol: String,
    decimal_separator: String,
    grouping_separator: String,
    group_width: u32,
    leading_zero: bool,
    positive_order: usize,
    negative_order: usize,
    negative_sign: String,
}

#[derive(Deserialize, PartialEq, Eq)]
pub(super) struct CalendarConventions {
    pub(super) month_names: [String; 12],
    pub(super) abbreviated_month_names: [String; 12],
    pub(super) weekday_names: [String; 7],
    pub(super) abbreviated_weekday_names: [String; 7],
    pub(super) am_designator: String,
    pub(super) pm_designator: String,
    pub(super) era: String,
    pub(super) short_date: String,
    pub(super) time_format: String,
    pub(super) genitive_month_names: Option<[String; 12]>,
    pub(super) genitive_abbreviated_month_names: Option<[String; 12]>,
}

impl CalendarConventions {
    pub(super) fn month_name(&self, index: usize, abbreviated: bool, genitive: bool) -> &str {
        let (names, genitive_names) = if abbreviated {
            (
                &self.abbreviated_month_names,
                &self.genitive_abbreviated_month_names,
            )
        } else {
            (&self.month_names, &self.genitive_month_names)
        };
        let names = genitive_names
            .as_ref()
            .filter(|_| genitive)
            .unwrap_or(names);
        &names[index]
    }
}

#[derive(Deserialize)]
struct LocaleTable(Vec<WindowsLocaleConventions>, Vec<(u32, usize)>);

pub(super) fn bundled_locale_conventions(locale: u32) -> &'static WindowsLocaleConventions {
    static LOCALES: OnceLock<LocaleTable> = OnceLock::new();
    let locales = LOCALES.get_or_init(|| {
        serde_json::from_str(include_str!("locale_conventions.json"))
            .expect("bundled Windows locale conventions must be valid")
    });
    let find = |id| {
        let row = locales.1.binary_search_by_key(&id, |(id, _)| *id).ok()?;
        locales.0.get(locales.1[row].1)
    };
    find(locale)
        .or_else(|| find(1033))
        .expect("bundled Windows locale conventions contain US English")
}

pub(super) fn host_locale_conventions(locale: u32) -> Option<WindowsLocaleConventions> {
    #[cfg(windows)]
    return host_locale_conventions::read_windows_locale_conventions(locale);
    #[cfg(not(windows))]
    {
        let _ = locale;
        None
    }
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
        let negative = if include_cents {
            cents < 0
        } else {
            cents / 100 < 0
        };
        let pattern = if negative {
            NEGATIVE[self.negative_order]
        } else {
            POSITIVE[self.positive_order]
        };

        let absolute_cents = cents.unsigned_abs();
        let whole = absolute_cents / 100;
        for token in pattern.chars() {
            match token {
                '$' => target.write_str(symbol)?,
                '-' => target.write_str(&self.negative_sign)?,
                '#' => {
                    if whole != 0 || self.leading_zero || !include_cents {
                        self.write_grouped_whole_amount(whole, target)?;
                    }
                    if include_cents {
                        target.write_str(&self.decimal_separator)?;
                        write!(target, "{:02}", absolute_cents % 100)?;
                    }
                }
                other => target.write_char(other)?,
            }
        }
        Ok(())
    }

    fn write_grouped_whole_amount(&self, amount: u64, target: &mut impl fmt::Write) -> fmt::Result {
        if self.group_width == 0 {
            return write!(target, "{amount}");
        }
        let base = 10_u64.pow(self.group_width);
        let mut divisor = 1;
        while amount / divisor >= base {
            divisor *= base;
        }
        write!(target, "{}", amount / divisor)?;
        while divisor > 1 {
            divisor /= base;
            target.write_str(&self.grouping_separator)?;
            write!(
                target,
                "{:0width$}",
                amount / divisor % base,
                width = self.group_width as usize
            )?;
        }
        Ok(())
    }
}
