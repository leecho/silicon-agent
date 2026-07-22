//! 日历后端：CRUD trait + 真实 EventKit 实现 `EkCalendar` + 内存 mock `MockCalendar`。
//!
//! 日期统一用 ISO 8601 / RFC3339 字符串在边界传递，内部用 `chrono` 解析、`NSDate` 与 EventKit 交互。
//! v1 对重复事件只读：定位到的 EKEvent 若 `hasRecurrenceRules` 为真，update/delete 返回 `Unsupported`。

use super::AppleError;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EventItem {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub calendar: String,
    pub recurring: bool,
}

#[derive(Debug, Clone)]
pub struct EventDraft {
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub calendar: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EventPatch {
    pub title: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
}

pub trait CalendarBackend: Send + Sync {
    fn list_events(&self, start_iso: &str, end_iso: &str) -> Result<Vec<EventItem>, AppleError>;
    fn get_event(&self, id: &str) -> Result<EventItem, AppleError>;
    fn create_event(&self, draft: EventDraft) -> Result<EventItem, AppleError>;
    fn update_event(&self, id: &str, patch: EventPatch) -> Result<EventItem, AppleError>;
    fn delete_event(&self, id: &str) -> Result<(), AppleError>;
}

/// ISO8601/RFC3339 字符串 → unix 秒（f64）。容忍带/不带时区（无时区按 UTC 解释）。
pub(crate) fn iso_to_unix(s: &str) -> Result<f64, AppleError> {
    use chrono::{DateTime, NaiveDateTime, Utc};
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp() as f64 + dt.timestamp_subsec_nanos() as f64 / 1e9);
    }
    // 回退：无时区的 naive 形式，按 UTC 解释。
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            let dt = ndt.and_utc();
            return Ok(dt.timestamp() as f64 + dt.timestamp_subsec_nanos() as f64 / 1e9);
        }
    }
    let _ = Utc::now();
    Err(AppleError::Backend(format!("无法解析时间：{s}")))
}

/// unix 秒（f64）→ RFC3339（UTC，毫秒精度）。
pub(crate) fn unix_to_iso(t: f64) -> String {
    use chrono::{TimeZone, Utc};
    let secs = t.floor() as i64;
    let nanos = ((t - t.floor()) * 1e9).round() as u32;
    match Utc.timestamp_opt(secs, nanos) {
        chrono::LocalResult::Single(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        _ => Utc
            .timestamp_opt(secs, 0)
            .single()
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_default(),
    }
}

// ===================== 真实 EventKit 实现 =====================

use super::SendStore;
use objc2::rc::Retained;
use objc2_event_kit::{EKCalendar, EKEntityType, EKEvent, EKSpan};
use objc2_foundation::{NSDate, NSString};

pub struct EkCalendar {
    store: SendStore,
}

impl Default for EkCalendar {
    fn default() -> Self {
        Self::new()
    }
}

impl EkCalendar {
    pub fn new() -> Self {
        Self { store: SendStore::new() }
    }

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    /// 解析 NSError，能识别授权类错误就返回 PermissionDenied，否则 Backend。
    fn map_err(err: &objc2_foundation::NSError) -> AppleError {
        let domain = err.domain().to_string();
        let code = err.code();
        let msg = err.localizedDescription().to_string();
        // EKError / 授权失败的粗略判定。
        let denied = msg.contains("authoriz")
            || msg.contains("permission")
            || msg.contains("授权")
            || msg.contains("denied")
            || (domain.contains("EKError") && code == 8); // EKErrorAccessDenied 区间，做兜底
        if denied {
            AppleError::PermissionDenied
        } else {
            AppleError::Backend(format!("{msg}（{domain} #{code}）"))
        }
    }

    /// 从一个 EKEvent 读出 EventItem。
    fn to_item(ev: &EKEvent) -> EventItem {
        let id = unsafe { ev.eventIdentifier() }
            .map(|s| s.to_string())
            .unwrap_or_default();
        let title = unsafe { ev.title() }.to_string();
        let start = unix_to_iso(unsafe { ev.startDate() }.timeIntervalSince1970());
        let end = unix_to_iso(unsafe { ev.endDate() }.timeIntervalSince1970());
        let all_day = unsafe { ev.isAllDay() };
        let location = unsafe { ev.location() }.map(|s| s.to_string());
        let notes = unsafe { ev.notes() }.map(|s| s.to_string());
        let calendar = unsafe { ev.calendar() }
            .map(|c| unsafe { c.title() }.to_string())
            .unwrap_or_default();
        let recurring = unsafe { ev.hasRecurrenceRules() };
        EventItem { id, title, start, end, all_day, location, notes, calendar, recurring }
    }

    /// 按名称在日历列表里找一个匹配日历。
    fn find_calendar_by_name(&self, name: &str) -> Option<Retained<EKCalendar>> {
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Event) };
        let count = cals.count();
        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            if unsafe { cal.title() }.to_string() == name {
                return Some(cal);
            }
        }
        None
    }

    fn locate(&self, id: &str) -> Result<Retained<EKEvent>, AppleError> {
        let nsid = Self::ns(id);
        unsafe { self.store.eventWithIdentifier(&nsid) }
            .ok_or_else(|| AppleError::NotFound(id.to_string()))
    }
}

impl CalendarBackend for EkCalendar {
    fn list_events(&self, start_iso: &str, end_iso: &str) -> Result<Vec<EventItem>, AppleError> {
        let start = iso_to_unix(start_iso)?;
        let end = iso_to_unix(end_iso)?;
        let start_date = NSDate::dateWithTimeIntervalSince1970(start);
        let end_date = NSDate::dateWithTimeIntervalSince1970(end);
        let predicate = unsafe {
            self.store
                .predicateForEventsWithStartDate_endDate_calendars(&start_date, &end_date, None)
        };
        let events = unsafe { self.store.eventsMatchingPredicate(&predicate) };
        let count = events.count();
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let ev = events.objectAtIndex(i);
            out.push(Self::to_item(&ev));
        }
        Ok(out)
    }

    fn get_event(&self, id: &str) -> Result<EventItem, AppleError> {
        let ev = self.locate(id)?;
        Ok(Self::to_item(&ev))
    }

    fn create_event(&self, draft: EventDraft) -> Result<EventItem, AppleError> {
        let start = iso_to_unix(&draft.start)?;
        let end = iso_to_unix(&draft.end)?;
        let ev = unsafe { EKEvent::eventWithEventStore(&self.store) };
        unsafe {
            ev.setTitle(Some(&Self::ns(&draft.title)));
            ev.setStartDate(Some(&NSDate::dateWithTimeIntervalSince1970(start)));
            ev.setEndDate(Some(&NSDate::dateWithTimeIntervalSince1970(end)));
            ev.setAllDay(draft.all_day);
            if let Some(loc) = &draft.location {
                ev.setLocation(Some(&Self::ns(loc)));
            }
            if let Some(notes) = &draft.notes {
                ev.setNotes(Some(&Self::ns(notes)));
            }
        }
        // 日历：指定名称就找，否则默认日历。
        let cal = match &draft.calendar {
            Some(name) => self
                .find_calendar_by_name(name)
                .ok_or_else(|| AppleError::NotFound(format!("日历：{name}")))?,
            None => unsafe { self.store.defaultCalendarForNewEvents() }
                .ok_or_else(|| AppleError::Backend("无可用默认日历".into()))?,
        };
        unsafe { ev.setCalendar(Some(&cal)) };

        unsafe { self.store.saveEvent_span_error(&ev, EKSpan::ThisEvent) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(Self::to_item(&ev))
    }

    fn update_event(&self, id: &str, patch: EventPatch) -> Result<EventItem, AppleError> {
        let ev = self.locate(id)?;
        if unsafe { ev.hasRecurrenceRules() } {
            return Err(AppleError::Unsupported("重复事件 v1 只读".into()));
        }
        unsafe {
            if let Some(t) = &patch.title {
                ev.setTitle(Some(&Self::ns(t)));
            }
            if let Some(s) = &patch.start {
                let secs = iso_to_unix(s)?;
                ev.setStartDate(Some(&NSDate::dateWithTimeIntervalSince1970(secs)));
            }
            if let Some(e) = &patch.end {
                let secs = iso_to_unix(e)?;
                ev.setEndDate(Some(&NSDate::dateWithTimeIntervalSince1970(secs)));
            }
            if let Some(loc) = &patch.location {
                ev.setLocation(Some(&Self::ns(loc)));
            }
            if let Some(notes) = &patch.notes {
                ev.setNotes(Some(&Self::ns(notes)));
            }
        }
        unsafe { self.store.saveEvent_span_error(&ev, EKSpan::ThisEvent) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(Self::to_item(&ev))
    }

    fn delete_event(&self, id: &str) -> Result<(), AppleError> {
        let ev = self.locate(id)?;
        if unsafe { ev.hasRecurrenceRules() } {
            return Err(AppleError::Unsupported("重复事件 v1 只读".into()));
        }
        unsafe { self.store.removeEvent_span_error(&ev, EKSpan::ThisEvent) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(())
    }
}

// ===================== 内存 mock =====================

use std::sync::Mutex;

pub struct MockCalendar {
    inner: Mutex<MockInner>,
}

struct MockInner {
    events: Vec<EventItem>,
    next: u64,
}

impl Default for MockCalendar {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCalendar {
    pub fn new() -> Self {
        Self { inner: Mutex::new(MockInner { events: Vec::new(), next: 1 }) }
    }

    /// 测试辅助：直接塞入一个事件（如重复事件），返回其 id。
    pub fn seed(&self, item: EventItem) -> String {
        let mut g = self.inner.lock().unwrap();
        let mut item = item;
        if item.id.is_empty() {
            item.id = format!("mock-{}", g.next);
            g.next += 1;
        }
        let id = item.id.clone();
        g.events.push(item);
        id
    }
}

impl CalendarBackend for MockCalendar {
    fn list_events(&self, start_iso: &str, end_iso: &str) -> Result<Vec<EventItem>, AppleError> {
        let start = iso_to_unix(start_iso)?;
        let end = iso_to_unix(end_iso)?;
        let g = self.inner.lock().unwrap();
        let mut out = Vec::new();
        for ev in &g.events {
            let es = iso_to_unix(&ev.start)?;
            // 与查询区间有交集即纳入。
            let ee = iso_to_unix(&ev.end).unwrap_or(es);
            if ee >= start && es <= end {
                out.push(ev.clone());
            }
        }
        Ok(out)
    }

    fn get_event(&self, id: &str) -> Result<EventItem, AppleError> {
        let g = self.inner.lock().unwrap();
        g.events
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| AppleError::NotFound(id.to_string()))
    }

    fn create_event(&self, draft: EventDraft) -> Result<EventItem, AppleError> {
        // 校验时间可解析。
        iso_to_unix(&draft.start)?;
        iso_to_unix(&draft.end)?;
        let mut g = self.inner.lock().unwrap();
        let id = format!("mock-{}", g.next);
        g.next += 1;
        let item = EventItem {
            id: id.clone(),
            title: draft.title,
            start: draft.start,
            end: draft.end,
            all_day: draft.all_day,
            location: draft.location,
            notes: draft.notes,
            calendar: draft.calendar.unwrap_or_else(|| "默认".into()),
            recurring: false,
        };
        g.events.push(item.clone());
        Ok(item)
    }

    fn update_event(&self, id: &str, patch: EventPatch) -> Result<EventItem, AppleError> {
        let mut g = self.inner.lock().unwrap();
        let ev = g
            .events
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| AppleError::NotFound(id.to_string()))?;
        if ev.recurring {
            return Err(AppleError::Unsupported("重复事件 v1 只读".into()));
        }
        if let Some(t) = patch.title {
            ev.title = t;
        }
        if let Some(s) = patch.start {
            iso_to_unix(&s)?;
            ev.start = s;
        }
        if let Some(e) = patch.end {
            iso_to_unix(&e)?;
            ev.end = e;
        }
        if let Some(loc) = patch.location {
            ev.location = Some(loc);
        }
        if let Some(notes) = patch.notes {
            ev.notes = Some(notes);
        }
        Ok(ev.clone())
    }

    fn delete_event(&self, id: &str) -> Result<(), AppleError> {
        let mut g = self.inner.lock().unwrap();
        if let Some(ev) = g.events.iter().find(|e| e.id == id) {
            if ev.recurring {
                return Err(AppleError::Unsupported("重复事件 v1 只读".into()));
            }
        } else {
            return Err(AppleError::NotFound(id.to_string()));
        }
        g.events.retain(|e| e.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_unix_roundtrip() {
        let t = iso_to_unix("2026-06-27T10:30:00Z").unwrap();
        let s = unix_to_iso(t);
        let t2 = iso_to_unix(&s).unwrap();
        assert!((t - t2).abs() < 0.001, "roundtrip drift: {t} vs {t2}");
        // 已知值校验。
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z").unwrap(), 0.0);
    }

    #[test]
    fn iso_parse_with_offset_and_naive() {
        let a = iso_to_unix("2026-06-27T12:00:00+02:00").unwrap();
        let b = iso_to_unix("2026-06-27T10:00:00Z").unwrap();
        assert!((a - b).abs() < 0.001);
        // naive 按 UTC。
        let c = iso_to_unix("2026-06-27T10:00:00").unwrap();
        assert!((c - b).abs() < 0.001);
    }

    #[test]
    fn iso_parse_error() {
        assert!(matches!(iso_to_unix("not-a-date"), Err(AppleError::Backend(_))));
    }

    fn draft() -> EventDraft {
        EventDraft {
            title: "会议".into(),
            start: "2026-06-27T09:00:00Z".into(),
            end: "2026-06-27T10:00:00Z".into(),
            all_day: false,
            location: Some("3 楼".into()),
            notes: None,
            calendar: None,
        }
    }

    #[test]
    fn mock_crud_roundtrip() {
        let cal = MockCalendar::new();
        let created = cal.create_event(draft()).unwrap();
        assert_eq!(created.title, "会议");
        assert!(!created.recurring);

        let got = cal.get_event(&created.id).unwrap();
        assert_eq!(got, created);

        let updated = cal
            .update_event(
                &created.id,
                EventPatch { title: Some("周会".into()), notes: Some("议程".into()), ..Default::default() },
            )
            .unwrap();
        assert_eq!(updated.title, "周会");
        assert_eq!(updated.notes.as_deref(), Some("议程"));

        let listed = cal.list_events("2026-06-27T00:00:00Z", "2026-06-28T00:00:00Z").unwrap();
        assert_eq!(listed.len(), 1);
        // 区间外不返回。
        let none = cal.list_events("2030-01-01T00:00:00Z", "2030-01-02T00:00:00Z").unwrap();
        assert!(none.is_empty());

        cal.delete_event(&created.id).unwrap();
        assert!(matches!(cal.get_event(&created.id), Err(AppleError::NotFound(_))));
    }

    #[test]
    fn mock_get_missing_not_found() {
        let cal = MockCalendar::new();
        assert!(matches!(cal.get_event("nope"), Err(AppleError::NotFound(_))));
        assert!(matches!(cal.delete_event("nope"), Err(AppleError::NotFound(_))));
        assert!(matches!(
            cal.update_event("nope", EventPatch::default()),
            Err(AppleError::NotFound(_))
        ));
    }

    #[test]
    fn mock_recurring_is_readonly() {
        let cal = MockCalendar::new();
        let id = cal.seed(EventItem {
            id: String::new(),
            title: "每日站会".into(),
            start: "2026-06-27T09:00:00Z".into(),
            end: "2026-06-27T09:15:00Z".into(),
            all_day: false,
            location: None,
            notes: None,
            calendar: "工作".into(),
            recurring: true,
        });
        assert!(matches!(
            cal.update_event(&id, EventPatch { title: Some("x".into()), ..Default::default() }),
            Err(AppleError::Unsupported(_))
        ));
        assert!(matches!(cal.delete_event(&id), Err(AppleError::Unsupported(_))));
        // 仍可读。
        assert!(cal.get_event(&id).is_ok());
    }
}
