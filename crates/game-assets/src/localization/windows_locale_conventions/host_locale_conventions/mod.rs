//! Reads a Windows locale's current settings without changing them.

use super::{CalendarConventions, CurrencyConventions, WindowsLocaleConventions};
use std::ptr;

#[allow(unsafe_code, reason = "declares the Windows locale information API")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocaleInfoW(locale: u32, field: u32, buffer: *mut u16, buffer_length: i32) -> i32;
    fn GetCalendarInfoW(
        locale: u32,
        calendar: u32,
        field: u32,
        buffer: *mut u16,
        buffer_length: i32,
        number: *mut u32,
    ) -> i32;
}

pub(super) fn read_windows_locale_conventions(locale: u32) -> Option<WindowsLocaleConventions> {
    use super::bundled_locale_conventions;
    let fallback = bundled_locale_conventions(locale);
    let numeric = |field| read_locale_field(locale, field)?.parse::<u32>().ok();
    let grouping = read_locale_field(locale, 24)?;
    let group_width = grouping.split(';').next()?.parse::<u32>().ok()?;
    let positive_order = numeric(27)? as usize;
    let negative_order = numeric(28)? as usize;
    if group_width > 9 || positive_order > 3 || negative_order > 16 {
        return None;
    }
    let currency = CurrencyConventions {
        symbol: read_locale_field(locale, 20)?,
        decimal_separator: read_locale_field(locale, 22)?,
        grouping_separator: read_locale_field(locale, 23)?,
        group_width,
        leading_zero: numeric(18)? != 0,
        positive_order,
        negative_order,
        negative_sign: read_locale_field(locale, 81)?,
    };
    let names = &fallback.calendar;
    const CAL_RETURN_GENITIVE_NAMES: u32 = 0x10000000;
    let genitive_names = |field, fallback: &Option<[String; 12]>| {
        let names: [Option<String>; 12] = std::array::from_fn(|index| {
            read_calendar_field(locale, (field + index as u32) | CAL_RETURN_GENITIVE_NAMES)
        });
        if names
            .iter()
            .all(|name| name.as_ref().is_some_and(|name| !name.is_empty()))
        {
            Some(names.map(Option::unwrap_or_default))
        } else {
            fallback.clone()
        }
    };
    let day_name =
        |field: u32, index: usize| read_calendar_field(locale, field + ((index + 6) % 7) as u32);
    let calendar = CalendarConventions {
        month_names: std::array::from_fn(|index| {
            read_calendar_field(locale, 21 + index as u32)
                .unwrap_or_else(|| names.month_names[index].clone())
        }),
        abbreviated_month_names: std::array::from_fn(|index| {
            read_calendar_field(locale, 34 + index as u32)
                .unwrap_or_else(|| names.abbreviated_month_names[index].clone())
        }),
        weekday_names: std::array::from_fn(|index| {
            day_name(7, index).unwrap_or_else(|| names.weekday_names[index].clone())
        }),
        abbreviated_weekday_names: std::array::from_fn(|index| {
            day_name(14, index).unwrap_or_else(|| names.abbreviated_weekday_names[index].clone())
        }),
        am_designator: read_locale_field(locale, 40).unwrap_or_else(|| names.am_designator.clone()),
        pm_designator: read_locale_field(locale, 41).unwrap_or_else(|| names.pm_designator.clone()),
        era: read_calendar_field(locale, 4).unwrap_or_else(|| names.era.clone()),
        short_date: read_calendar_field(locale, 5).unwrap_or_else(|| names.short_date.clone()),
        time_format: read_locale_field(locale, 4099).unwrap_or_else(|| names.time_format.clone()),
        genitive_month_names: genitive_names(21, &names.genitive_month_names),
        genitive_abbreviated_month_names: genitive_names(
            34,
            &names.genitive_abbreviated_month_names,
        ),
    };
    Some(WindowsLocaleConventions { currency, calendar })
}

#[allow(
    unsafe_code,
    reason = "reads Windows locale fields into bounded UTF-16 storage"
)]
fn read_locale_field(locale: u32, field: u32) -> Option<String> {
    // SAFETY: The buffer and capacity always refer to the same writable array.
    read_utf16_field(|buffer, length| unsafe { GetLocaleInfoW(locale, field, buffer, length) })
}

#[allow(
    unsafe_code,
    reason = "reads Gregorian calendar fields into bounded UTF-16 storage"
)]
fn read_calendar_field(locale: u32, field: u32) -> Option<String> {
    // SAFETY: The buffer is bounded by length and these fields return strings.
    read_utf16_field(|buffer, length| unsafe {
        GetCalendarInfoW(locale, 1, field, buffer, length, ptr::null_mut())
    })
}

fn read_utf16_field(read: impl FnOnce(*mut u16, i32) -> i32) -> Option<String> {
    let mut buffer = [0; 256];
    let length = usize::try_from(read(buffer.as_mut_ptr(), 256))
        .ok()?
        .checked_sub(1)?;
    String::from_utf16(buffer.get(..length)?).ok()
}
