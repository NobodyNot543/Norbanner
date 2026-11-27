//! Month grids for the dashboard's calendar (proleptic Gregorian).

#[must_use]
pub const fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[must_use]
pub const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Day of the week, 0 = Sunday … 6 = Saturday (Sakamoto's method).
#[must_use]
pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let m = month.clamp(1, 12) as usize;
    let w = (y + y / 4 - y / 100 + y / 400 + T[m - 1] + day as i32) % 7;
    w.rem_euclid(7) as u32
}

/// `(year, month)` moved by `delta` months.
#[must_use]
pub fn add_months(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year * 12 + month as i32 - 1 + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

/// One cell of a month grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub day: u32,
    /// Whether the day belongs to the month shown (not padding).
    pub in_month: bool,
}

/// Six weeks of cells covering `month`, weeks starting on `first_weekday`
/// (0 = Sunday).
#[must_use]
pub fn grid(year: i32, month: u32, first_weekday: u32) -> Vec<Cell> {
    let lead = (weekday(year, month, 1) + 7 - first_weekday % 7) % 7;
    let (py, pm) = add_months(year, month, -1);
    let previous_days = days_in_month(py, pm);
    let days = days_in_month(year, month);
    let mut cells = Vec::with_capacity(42);
    for i in 0..lead {
        cells.push(Cell {
            day: previous_days - lead + 1 + i,
            in_month: false,
        });
    }
    for day in 1..=days {
        cells.push(Cell {
            day,
            in_month: true,
        });
    }
    let mut next = 1;
    while cells.len() < 42 {
        cells.push(Cell {
            day: next,
            in_month: false,
        });
        next += 1;
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        assert_eq!(weekday(2026, 11, 25), 3); // Wednesday
        assert_eq!(weekday(2000, 1, 1), 6); // Saturday
        assert_eq!(weekday(2024, 2, 29), 4); // Thursday
    }

    #[test]
    fn grid_starts_on_requested_weekday() {
        // November 2026 starts on a Sunday.
        let sunday_first = grid(2026, 11, 0);
        assert_eq!(
            sunday_first[0],
            Cell {
                day: 1,
                in_month: true
            }
        );
        let monday_first = grid(2026, 11, 1);
        assert_eq!(
            monday_first[6],
            Cell {
                day: 1,
                in_month: true
            }
        );
        assert_eq!(
            monday_first[0],
            Cell {
                day: 26,
                in_month: false
            }
        );
        assert_eq!(monday_first.len(), 42);
    }

    #[test]
    fn month_arithmetic() {
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
    }
}
