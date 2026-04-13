use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

pub enum Freq {
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

pub struct Rrule {
    pub freq: Freq,
    pub interval: u32,
    pub byday: Option<String>,
    pub bymonthday: Option<u32>,
    pub bymonth: Option<u32>,
}

pub fn parse_rrule(s: &str) -> Option<Rrule> {
    let s = s.strip_prefix("RRULE:").unwrap_or(s);

    let mut freq = None;
    let mut interval = 1u32;
    let mut byday = None;
    let mut bymonthday = None;
    let mut bymonth = None;

    for part in s.split(';') {
        let mut kv = part.splitn(2, '=');
        let key = kv.next()?.trim();
        let val = kv.next().unwrap_or("").trim();
        match key {
            "FREQ" => {
                freq = Some(match val {
                    "HOURLY" => Freq::Hourly,
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                })
            }
            "INTERVAL" => interval = val.parse().unwrap_or(1),
            "BYDAY" => byday = Some(val.to_string()),
            "BYMONTHDAY" => bymonthday = val.parse().ok(),
            "BYMONTH" => bymonth = val.parse().ok(),
            _ => {}
        }
    }

    Some(Rrule { freq: freq?, interval, byday, bymonthday, bymonth })
}

pub fn next_occurrence(rrule: &Rrule, from: NaiveDateTime) -> Option<NaiveDateTime> {
    match rrule.freq {
        Freq::Hourly => Some(from + Duration::hours(rrule.interval as i64)),
        Freq::Daily => Some(from + Duration::days(rrule.interval as i64)),
        Freq::Weekly => Some(next_weekly(rrule, from)),
        Freq::Monthly => next_monthly(rrule, from),
        Freq::Yearly => next_yearly(rrule, from),
    }
}

fn next_weekly(rrule: &Rrule, from: NaiveDateTime) -> NaiveDateTime {
    let from_date = from.date();

    if let Some(byday) = &rrule.byday {
        let weekdays: Vec<Weekday> = byday
            .split(',')
            .filter_map(|s| parse_weekday_code(s.trim()))
            .collect();

        if weekdays.is_empty() {
            return from + Duration::weeks(rrule.interval as i64);
        }

        let from_wd = from_date.weekday();
        let min_days = weekdays
            .iter()
            .map(|&wd| {
                let d = days_until_weekday(from_wd, wd);
                if d == 0 { 7u32 } else { d }
            })
            .min()
            .unwrap_or(7);

        let extra_weeks = Duration::weeks((rrule.interval - 1) as i64);
        let next_date = from_date + Duration::days(min_days as i64) + extra_weeks;
        next_date.and_time(from.time())
    } else {
        from + Duration::weeks(rrule.interval as i64)
    }
}

fn next_monthly(rrule: &Rrule, from: NaiveDateTime) -> Option<NaiveDateTime> {
    if let Some(byday) = &rrule.byday {
        if let Some((n, wd)) = parse_ordinal_weekday(byday) {
            return next_nth_weekday_of_month(from, n, wd, rrule.interval);
        }
    }
    let target_day = rrule.bymonthday.unwrap_or(from.date().day());
    add_months_with_day(from, rrule.interval, target_day)
}

fn next_yearly(rrule: &Rrule, from: NaiveDateTime) -> Option<NaiveDateTime> {
    let target_month = rrule.bymonth.unwrap_or(from.date().month());

    if let Some(byday) = &rrule.byday {
        if let Some((n, wd)) = parse_ordinal_weekday(byday) {
            let year = from.date().year() + rrule.interval as i32;
            return nth_weekday_in_month(year, target_month, n, wd)
                .map(|d| d.and_time(from.time()));
        }
    }

    let target_day = rrule.bymonthday.unwrap_or(from.date().day());
    let year = from.date().year() + rrule.interval as i32;
    let max_day = days_in_month(year, target_month);
    NaiveDate::from_ymd_opt(year, target_month, target_day.min(max_day))
        .map(|d| d.and_time(from.time()))
}

fn add_months_with_day(from: NaiveDateTime, months: u32, day: u32) -> Option<NaiveDateTime> {
    let from_date = from.date();
    let total = (from_date.year() as u32) * 12 + from_date.month() - 1 + months;
    let year = (total / 12) as i32;
    let month = total % 12 + 1;
    let max_day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day.min(max_day))
        .map(|d| d.and_time(from.time()))
}

fn next_nth_weekday_of_month(
    from: NaiveDateTime,
    n: i32,
    wd: Weekday,
    interval: u32,
) -> Option<NaiveDateTime> {
    let from_date = from.date();
    let total = (from_date.year() as u32) * 12 + from_date.month() - 1 + interval;
    let year = (total / 12) as i32;
    let month = total % 12 + 1;
    nth_weekday_in_month(year, month, n, wd).map(|d| d.and_time(from.time()))
}

fn nth_weekday_in_month(year: i32, month: u32, n: i32, wd: Weekday) -> Option<NaiveDate> {
    if n > 0 {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let d = days_until_weekday(first.weekday(), wd);
        let first_occ = first + Duration::days(d as i64);
        Some(first_occ + Duration::weeks((n - 1) as i64))
    } else {
        let last = NaiveDate::from_ymd_opt(year, month, days_in_month(year, month))?;
        let d = days_back_to_weekday(last.weekday(), wd);
        let last_occ = last - Duration::days(d as i64);
        Some(last_occ - Duration::weeks((-n - 1) as i64))
    }
}

fn days_until_weekday(from: Weekday, target: Weekday) -> u32 {
    let f = from.num_days_from_monday();
    let t = target.num_days_from_monday();
    if t >= f { t - f } else { 7 + t - f }
}

fn days_back_to_weekday(from: Weekday, target: Weekday) -> u32 {
    let f = from.num_days_from_monday();
    let t = target.num_days_from_monday();
    if f >= t { f - t } else { 7 + f - t }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    if month == 12 {
        31
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
            .and_then(|d| d.pred_opt())
            .map(|d| d.day())
            .unwrap_or(30)
    }
}

fn parse_weekday_code(s: &str) -> Option<Weekday> {
    match s {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

fn parse_ordinal_weekday(s: &str) -> Option<(i32, Weekday)> {
    let s = s.trim();
    if s.len() < 3 {
        return None;
    }
    let wd_str = &s[s.len() - 2..];
    let n_str = &s[..s.len() - 2];
    let n: i32 = n_str.parse().ok()?;
    let wd = parse_weekday_code(wd_str)?;
    Some((n, wd))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(year: i32, month: u32, day: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    #[test]
    fn proxima_ocorrencia_diaria() {
        let rrule = parse_rrule("FREQ=DAILY;INTERVAL=1").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 20)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 4, 21).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_semanal_segunda() {
        // 2026-04-20 é segunda-feira; próxima segunda = 2026-04-27
        let rrule = parse_rrule("FREQ=WEEKLY;INTERVAL=1;BYDAY=MO").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 20)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 4, 27).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_semanal_proxima_sexta_a_partir_de_segunda() {
        // 2026-04-20 é segunda; próxima sexta = 2026-04-24
        let rrule = parse_rrule("FREQ=WEEKLY;INTERVAL=1;BYDAY=FR").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 20)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 4, 24).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_mensal_dia_15_antes_do_dia() {
        // from=2026-04-20 → dia 15 do próximo mês = 2026-05-15
        let rrule = parse_rrule("FREQ=MONTHLY;INTERVAL=1;BYMONTHDAY=15").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 20)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 5, 15).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_mensal_dia_31_em_fevereiro_clamp() {
        // BYMONTHDAY=31, from=2026-01-31 → fevereiro não tem 31 → 2026-02-28
        let rrule = parse_rrule("FREQ=MONTHLY;INTERVAL=1;BYMONTHDAY=31").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 1, 31)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_mensal_primeiro_domingo() {
        // BYDAY=1SU, from=2026-04-20 → 1º domingo de maio = 2026-05-03
        let rrule = parse_rrule("FREQ=MONTHLY;INTERVAL=1;BYDAY=1SU").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 20)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 5, 3).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_anual() {
        // from=2026-04-13 → próximo ano mesmo dia = 2027-04-13
        let rrule = parse_rrule("FREQ=YEARLY;INTERVAL=1;BYMONTH=4;BYMONTHDAY=13").unwrap();
        let next = next_occurrence(&rrule, dt(2026, 4, 13)).unwrap();
        assert_eq!(next.date(), NaiveDate::from_ymd_opt(2027, 4, 13).unwrap());
    }

    #[test]
    fn proxima_ocorrencia_hora() {
        let rrule = parse_rrule("FREQ=HOURLY;INTERVAL=1").unwrap();
        let from = dt(2026, 4, 20);
        let next = next_occurrence(&rrule, from).unwrap();
        assert_eq!(next, from + Duration::hours(1));
    }

    #[test]
    fn parse_rrule_com_prefixo_rrule() {
        let rrule = parse_rrule("RRULE:FREQ=DAILY;INTERVAL=2");
        assert!(rrule.is_some());
        let rrule = rrule.unwrap();
        assert_eq!(rrule.interval, 2);
    }

    #[test]
    fn parse_rrule_freq_invalida_retorna_none() {
        assert!(parse_rrule("FREQ=INVALID").is_none());
    }
}
