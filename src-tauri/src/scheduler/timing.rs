use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};

/// 私有泛型辅助：给定已解析的 after 时间，返回严格大于它的下一个触发 epoch 秒。
fn next_after_impl<Z: chrono::TimeZone>(spec: &str, after: DateTime<Z>) -> Result<i64, String> {
    let schedule = cron::Schedule::from_str(spec).map_err(|e| format!("cron 解析失败：{e}"))?;
    schedule
        .after(&after)
        .next()
        .map(|dt| dt.timestamp())
        .ok_or_else(|| "无下一个触发时间".into())
}

/// 给定 6 字段 cron 与 after（epoch 秒，UTC），返回严格大于 after 的下一个触发 epoch 秒。
/// 测试用 UTC；生产入口 `next_after_local` 用本地时区（见下）。
pub fn next_after_utc(spec: &str, after_secs: i64) -> Result<i64, String> {
    let after = Utc
        .timestamp_opt(after_secs, 0)
        .single()
        .ok_or("after 时间戳非法")?;
    next_after_impl(spec, after)
}

/// 生产用：按系统本地时区计算下一次触发（用户预设的 09:00 指本地 9 点）。
pub fn next_after_local(spec: &str, after_secs: i64) -> Result<i64, String> {
    use chrono::Local;
    let after = Local
        .timestamp_opt(after_secs, 0)
        .single()
        .ok_or("after 时间戳非法")?;
    next_after_impl(spec, after)
}

/// 启动期对单个任务的规划结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupPlan {
    /// 是否需要补跑一次（错过多槽也只补一次）。
    pub fire: bool,
    /// 重算后的 next_run_at（epoch 秒）。
    pub new_next: i64,
}

/// 启动规划（测试版用 UTC）。current_next 为 None 表示首次启用。
pub fn plan_startup_one(
    spec: &str,
    current_next: Option<i64>,
    now: i64,
) -> Result<StartupPlan, String> {
    match current_next {
        None => Ok(StartupPlan {
            fire: false,
            new_next: next_after_utc(spec, now)?,
        }),
        Some(next) if next <= now => Ok(StartupPlan {
            fire: true,
            new_next: next_after_utc(spec, now)?,
        }),
        Some(next) => Ok(StartupPlan {
            fire: false,
            new_next: next,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-01-01 00:00:00 UTC = 1767225600。测试用 UTC 固定点，避免本地时区漂移。
    // 注：生产用 Local；测试通过 next_after_utc 直接验证 cron 推进逻辑。
    #[test]
    fn next_after_advances_to_next_slot() {
        // 每天 09:00 的 6 字段 cron。
        let spec = "0 0 9 * * *";
        // 2026-01-01 08:00:00 UTC
        let after = 1767254400;
        let next = next_after_utc(spec, after).unwrap();
        // 应推进到 2026-01-01 09:00:00 UTC = 1767258000
        assert_eq!(next, 1767258000);
    }

    #[test]
    fn next_after_skips_to_following_day_when_past() {
        let spec = "0 0 9 * * *";
        // 2026-01-01 10:00:00 UTC，已过当天 9 点
        let after = 1767261600;
        let next = next_after_utc(spec, after).unwrap();
        // 次日 2026-01-02 09:00:00 UTC = 1767344400
        assert_eq!(next, 1767344400);
    }

    #[test]
    fn startup_plan_fires_once_for_missed_and_reschedules() {
        // next_run_at 已过去 → 补跑一次 + 重排到 now 之后。
        let now = 1767261600; // 已过 9 点
        let plan = plan_startup_one("0 0 9 * * *", Some(1767258000), now).unwrap();
        assert!(plan.fire, "错过的槽位应补跑一次");
        assert_eq!(plan.new_next, 1767344400, "重排到 now 之后的下一槽");
    }

    #[test]
    fn startup_plan_does_not_fire_when_future() {
        let now = 1767254400; // 08:00，未到 9 点
        let plan = plan_startup_one("0 0 9 * * *", Some(1767258000), now).unwrap();
        assert!(!plan.fire, "未来槽位不补跑");
        assert_eq!(plan.new_next, 1767258000, "保持原 next");
    }

    #[test]
    fn startup_plan_initializes_null_next() {
        let now = 1767254400;
        let plan = plan_startup_one("0 0 9 * * *", None, now).unwrap();
        assert!(!plan.fire, "首次启用不补跑");
        assert_eq!(plan.new_next, 1767258000, "从 now 计算首个 next");
    }

    #[test]
    fn next_after_rejects_invalid_spec() {
        assert!(
            next_after_utc("not a cron", 0).is_err(),
            "无效 cron 表达式应返回错误"
        );
    }

    #[test]
    fn next_after_rejects_overflow_timestamp() {
        assert!(
            next_after_utc("0 0 9 * * *", i64::MAX).is_err(),
            "溢出时间戳应返回错误"
        );
    }
}
