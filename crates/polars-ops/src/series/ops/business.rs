use polars_core::prelude::arity::binary_elementwise_values;
use polars_core::prelude::*;

/// Count the number of business days between `start` and `end`, excluding `end`.
///
/// # Arguments
/// - `start`: Series holding start dates.
/// - `end`: Series holding end dates.
/// - `week_mask`: A boolean array of length 7, where `true` indicates that the day is a business day.
/// - `holidays`: timestamps that are holidays. Must be provided as i32, i.e. the number of
///   days since the UNIX epoch.
pub fn business_day_count(
    start: &Series,
    end: &Series,
    week_mask: [bool; 7],
    holidays: &[i32],
) -> PolarsResult<Series> {
    if !week_mask.iter().any(|&x| x) {
        polars_bail!(ComputeError:"`week_mask` must have at least one business day");
    }

    let holidays = normalise_holidays(holidays, &week_mask);
    let start_dates = start.date()?;
    let end_dates = end.date()?;
    let n_business_days_in_week_mask = week_mask.iter().filter(|&x| *x).count() as i32;

    let out = match (start_dates.len(), end_dates.len()) {
        (_, 1) => {
            if let Some(end_date) = end_dates.get(0) {
                start_dates.apply_values(|start_date| {
                    business_day_count_impl(
                        start_date,
                        end_date,
                        &week_mask,
                        n_business_days_in_week_mask,
                        &holidays,
                    )
                })
            } else {
                Int32Chunked::full_null(start_dates.name(), start_dates.len())
            }
        },
        (1, _) => {
            if let Some(start_date) = start_dates.get(0) {
                end_dates.apply_values(|end_date| {
                    business_day_count_impl(
                        start_date,
                        end_date,
                        &week_mask,
                        n_business_days_in_week_mask,
                        &holidays,
                    )
                })
            } else {
                Int32Chunked::full_null(start_dates.name(), end_dates.len())
            }
        },
        _ => binary_elementwise_values(start_dates, end_dates, |start_date, end_date| {
            business_day_count_impl(
                start_date,
                end_date,
                &week_mask,
                n_business_days_in_week_mask,
                &holidays,
            )
        }),
    };
    Ok(out.into_series())
}

/// Ported from:
/// https://github.com/numpy/numpy/blob/e59c074842e3f73483afa5ddef031e856b9fd313/numpy/_core/src/multiarray/datetime_busday.c#L355-L433
fn business_day_count_impl(
    mut start_date: i32,
    mut end_date: i32,
    week_mask: &[bool; 7],
    n_business_days_in_week_mask: i32,
    holidays: &[i32],
) -> i32 {
    let swapped = start_date > end_date;
    if swapped {
        (start_date, end_date) = (end_date, start_date);
        start_date += 1;
        end_date += 1;
    }

    let holidays_begin = match holidays.binary_search(&start_date) {
        Ok(x) => x,
        Err(x) => x,
    } as i32;
    let holidays_end = match holidays[(holidays_begin as usize)..].binary_search(&end_date) {
        Ok(x) => x as i32 + holidays_begin,
        Err(x) => x as i32 + holidays_begin,
    };

    let mut start_weekday = weekday(start_date);
    let diff = end_date - start_date;
    let whole_weeks = diff / 7;
    let mut count = -(holidays_end - holidays_begin);
    count += whole_weeks * n_business_days_in_week_mask;
    start_date += whole_weeks * 7;
    while start_date < end_date {
        // SAFETY: week_mask is length 7, start_weekday is between 0 and 6
        if unsafe { *week_mask.get_unchecked(start_weekday) } {
            count += 1;
        }
        start_date += 1;
        start_weekday = increment_weekday(start_weekday);
    }
    if swapped {
        -count
    } else {
        count
    }
}

/// Sort and deduplicate holidays and remove holidays that are not business days.
fn normalise_holidays(holidays: &[i32], week_mask: &[bool; 7]) -> Vec<i32> {
    let mut holidays: Vec<i32> = holidays.to_vec();
    holidays.sort_unstable();
    let mut previous_holiday: Option<i32> = None;
    holidays.retain(|&x| {
        // SAFETY: week_mask is length 7, start_weekday is between 0 and 6
        if (Some(x) == previous_holiday) || !unsafe { *week_mask.get_unchecked(weekday(x)) } {
            return false;
        }
        previous_holiday = Some(x);
        true
    });
    holidays
}

fn weekday(x: i32) -> usize {
    // the first modulo might return a negative number, so we add 7 and take
    // the modulo again so we're sure we have something between 0 (Monday)
    // and 6 (Sunday)
    (((x - 4) % 7 + 7) % 7) as usize
}

fn increment_weekday(x: usize) -> usize {
    if x == 6 {
        0
    } else {
        x + 1
    }
}
