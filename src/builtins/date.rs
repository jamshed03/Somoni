//! `format_date(millis, pattern)`: a moment as text in UTC, from the milliseconds since 1970 that `now()` gives.

/// Returns the moment `millis` in UTC as `pattern` with `YYYY`, `MM`, `DD`, `hh`, `mm` and `ss` filled in.
pub(super) fn format_date(millis: i64, pattern: &str) -> String {
    let seconds = millis.div_euclid(1000);
    let (year, month, day) = civil_date(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    pattern
        .replace("YYYY", &format!("{:04}", year))
        .replace("MM", &format!("{:02}", month))
        .replace("DD", &format!("{:02}", day))
        .replace("hh", &format!("{:02}", time / 3600))
        .replace("mm", &format!("{:02}", time % 3600 / 60))
        .replace("ss", &format!("{:02}", time % 60))
}

/// Returns year, month and day of the day `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
