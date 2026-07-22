use std::str::FromStr;

use serde::Deserialize;

/// 前端传入的调度描述：预设或原始 cron。统一归一化为 6 字段 cron 串。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleInput {
    Preset {
        preset: PresetKind,
        /// "HH:MM"（interval 时忽略）。
        #[serde(default)]
        time: String,
        /// 1=Mon .. 7=Sun（仅 weekly 用）。
        #[serde(default)]
        weekdays: Vec<u32>,
        /// 仅 interval 用。
        #[serde(default)]
        every: Option<Every>,
    },
    Cron {
        expr: String,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    Interval,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Every {
    pub value: u32,
    pub unit: IntervalUnit,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntervalUnit {
    Minutes,
    Hours,
}

const WEEKDAY_NAMES: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

/// 把 ScheduleInput 归一化为 6 字段 cron（sec min hour dom month dow），并校验可解析。
pub fn normalize_to_cron(input: &ScheduleInput) -> Result<String, String> {
    let spec = match input {
        ScheduleInput::Cron { expr } => {
            let trimmed = expr.trim();
            // 标准 5 字段 → 前置秒位 0；已是 6/7 字段则原样。
            let field_count = trimmed.split_whitespace().count();
            match field_count {
                5 => format!("0 {trimmed}"),
                6 => trimmed.to_string(),
                n => return Err(format!("cron 字段数非法：{n}（需 5 或 6 字段）")),
            }
        }
        ScheduleInput::Preset {
            preset,
            time,
            weekdays,
            every,
        } => match preset {
            PresetKind::Interval => {
                let every = every.as_ref().ok_or("interval 预设缺少 every")?;
                match every.unit {
                    IntervalUnit::Minutes => {
                        if !(1..=59).contains(&every.value) {
                            return Err("间隔分钟须在 1..=59".into());
                        }
                        format!("0 */{} * * * *", every.value)
                    }
                    IntervalUnit::Hours => {
                        if !(1..=23).contains(&every.value) {
                            return Err("间隔小时须在 1..=23".into());
                        }
                        format!("0 0 */{} * * *", every.value)
                    }
                }
            }
            PresetKind::Daily => {
                let (h, m) = parse_hhmm(time)?;
                format!("0 {m} {h} * * *")
            }
            PresetKind::Weekly => {
                let (h, m) = parse_hhmm(time)?;
                if weekdays.is_empty() {
                    return Err("weekly 预设须至少选一个星期几".into());
                }
                let mut names = Vec::new();
                for w in weekdays {
                    let idx = (*w as usize)
                        .checked_sub(1)
                        .filter(|i| *i < 7)
                        .ok_or_else(|| format!("weekday 越界：{w}（需 1..=7）"))?;
                    names.push(WEEKDAY_NAMES[idx]);
                }
                format!("0 {m} {h} * * {}", names.join(","))
            }
        },
    };
    // 校验：能被 cron crate 解析才返回。
    cron::Schedule::from_str(&spec).map_err(|e| format!("cron 解析失败：{e}"))?;
    Ok(spec)
}

fn parse_hhmm(time: &str) -> Result<(u32, u32), String> {
    let (h, m) = time
        .split_once(':')
        .ok_or_else(|| format!("时间格式应为 HH:MM：{time}"))?;
    let h: u32 = h.parse().map_err(|_| format!("小时非法：{time}"))?;
    let m: u32 = m.parse().map_err(|_| format!("分钟非法：{time}"))?;
    if h > 23 || m > 59 {
        return Err(format!("时间越界：{time}"));
    }
    Ok((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_preset_compiles_to_six_field_cron() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Daily,
            time: "09:00".into(),
            weekdays: vec![],
            every: None,
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 0 9 * * *");
    }

    #[test]
    fn weekly_preset_uses_weekday_names() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Weekly,
            time: "09:30".into(),
            weekdays: vec![1, 5], // Mon, Fri
            every: None,
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 30 9 * * MON,FRI");
    }

    #[test]
    fn interval_minutes_preset() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Interval,
            time: String::new(),
            weekdays: vec![],
            every: Some(Every {
                value: 30,
                unit: IntervalUnit::Minutes,
            }),
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 */30 * * * *");
    }

    #[test]
    fn interval_hours_preset() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Interval,
            time: String::new(),
            weekdays: vec![],
            every: Some(Every {
                value: 2,
                unit: IntervalUnit::Hours,
            }),
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 0 */2 * * *");
    }

    #[test]
    fn raw_cron_five_field_gets_seconds_prefix() {
        let input = ScheduleInput::Cron {
            expr: "30 9 * * 1-5".into(),
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 30 9 * * 1-5");
    }

    #[test]
    fn raw_cron_six_field_passthrough() {
        let input = ScheduleInput::Cron {
            expr: "0 30 9 * * 1-5".into(),
        };
        assert_eq!(normalize_to_cron(&input).unwrap(), "0 30 9 * * 1-5");
    }

    #[test]
    fn invalid_cron_is_rejected() {
        let input = ScheduleInput::Cron {
            expr: "not a cron".into(),
        };
        assert!(normalize_to_cron(&input).is_err());
    }

    #[test]
    fn weekday_out_of_range_rejected() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Weekly,
            time: "09:30".into(),
            weekdays: vec![8],
            every: None,
        };
        assert!(normalize_to_cron(&input).is_err());
    }

    #[test]
    fn weekday_zero_rejected() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Weekly,
            time: "09:30".into(),
            weekdays: vec![0],
            every: None,
        };
        assert!(normalize_to_cron(&input).is_err());
    }

    #[test]
    fn daily_time_out_of_range_rejected() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Daily,
            time: "24:00".into(),
            weekdays: vec![],
            every: None,
        };
        assert!(normalize_to_cron(&input).is_err());
    }

    #[test]
    fn interval_minutes_too_large_rejected() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Interval,
            time: String::new(),
            weekdays: vec![],
            every: Some(Every {
                value: 90,
                unit: IntervalUnit::Minutes,
            }),
        };
        assert!(normalize_to_cron(&input).is_err());
    }

    #[test]
    fn interval_hours_too_large_rejected() {
        let input = ScheduleInput::Preset {
            preset: PresetKind::Interval,
            time: String::new(),
            weekdays: vec![],
            every: Some(Every {
                value: 24,
                unit: IntervalUnit::Hours,
            }),
        };
        assert!(normalize_to_cron(&input).is_err());
    }
}
