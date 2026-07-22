//! 提醒事项后端：CRUD trait + 真实 EventKit 实现 `EkReminders` + 内存 mock `MockReminders`。
//!
//! EKReminder 是 EKCalendarItem 子类。截止时间来自 `dueDateComponents`（NSDateComponents），
//! 用 NSCalendar 与 NSDate 互转。列表查询用异步 fetch，经 mpsc 桥接为同步。

use super::AppleError;

use crate::apple::calendar::{iso_to_unix, unix_to_iso};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ReminderItem {
    pub id: String,
    pub title: String,
    pub due: Option<String>,
    pub completed: bool,
    pub list: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReminderDraft {
    pub title: String,
    pub due: Option<String>,
    pub notes: Option<String>,
    pub list: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ReminderPatch {
    pub title: Option<String>,
    pub due: Option<String>,
    pub notes: Option<String>,
}

pub trait RemindersBackend: Send + Sync {
    fn list_reminders(&self, include_completed: bool) -> Result<Vec<ReminderItem>, AppleError>;
    fn get_reminder(&self, id: &str) -> Result<ReminderItem, AppleError>;
    fn create_reminder(&self, draft: ReminderDraft) -> Result<ReminderItem, AppleError>;
    fn update_reminder(&self, id: &str, patch: ReminderPatch) -> Result<ReminderItem, AppleError>;
    fn complete_reminder(&self, id: &str) -> Result<ReminderItem, AppleError>;
    fn delete_reminder(&self, id: &str) -> Result<(), AppleError>;
}

// ===================== 真实 EventKit 实现 =====================

use super::SendStore;
use objc2::rc::Retained;
use objc2_event_kit::{EKCalendar, EKEntityType, EKReminder};
use objc2_foundation::{
    NSCalendar, NSCalendarUnit, NSDate, NSDateComponents, NSString,
};

pub struct EkReminders {
    store: SendStore,
}

impl Default for EkReminders {
    fn default() -> Self {
        Self::new()
    }
}

impl EkReminders {
    pub fn new() -> Self {
        Self { store: SendStore::new() }
    }

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    fn map_err(err: &objc2_foundation::NSError) -> AppleError {
        let domain = err.domain().to_string();
        let code = err.code();
        let msg = err.localizedDescription().to_string();
        let denied = msg.contains("authoriz")
            || msg.contains("permission")
            || msg.contains("授权")
            || msg.contains("denied");
        if denied {
            AppleError::PermissionDenied
        } else {
            AppleError::Backend(format!("{msg}（{domain} #{code}）"))
        }
    }

    /// NSDateComponents → ISO 字符串（经当前日历解析为绝对时刻）。
    fn components_to_iso(comps: &NSDateComponents) -> Option<String> {
        let cal = NSCalendar::currentCalendar();
        let date = cal.dateFromComponents(comps)?;
        Some(unix_to_iso(date.timeIntervalSince1970()))
    }

    /// ISO 字符串 → NSDateComponents（含年月日时分秒，便于 EventKit 当作有时间的 due）。
    fn iso_to_components(s: &str) -> Result<Retained<NSDateComponents>, AppleError> {
        let secs = iso_to_unix(s)?;
        let date = NSDate::dateWithTimeIntervalSince1970(secs);
        let cal = NSCalendar::currentCalendar();
        let units = NSCalendarUnit::Year
            | NSCalendarUnit::Month
            | NSCalendarUnit::Day
            | NSCalendarUnit::Hour
            | NSCalendarUnit::Minute
            | NSCalendarUnit::Second;
        let comps = cal.components_fromDate(units, &date);
        Ok(comps)
    }

    fn to_item(r: &EKReminder) -> ReminderItem {
        let id = unsafe { r.calendarItemIdentifier() }.to_string();
        let title = unsafe { r.title() }.to_string();
        let completed = unsafe { r.isCompleted() };
        let notes = unsafe { r.notes() }.map(|s| s.to_string());
        let list = unsafe { r.calendar() }
            .map(|c| unsafe { c.title() }.to_string())
            .unwrap_or_default();
        let due = unsafe { r.dueDateComponents() }
            .and_then(|c| Self::components_to_iso(&c));
        ReminderItem { id, title, due, completed, list, notes }
    }

    fn locate(&self, id: &str) -> Result<Retained<EKReminder>, AppleError> {
        let nsid = Self::ns(id);
        let item = unsafe { self.store.calendarItemWithIdentifier(&nsid) }
            .ok_or_else(|| AppleError::NotFound(id.to_string()))?;
        // 向下转型到 EKReminder。
        item.downcast::<EKReminder>()
            .map_err(|_| AppleError::NotFound(id.to_string()))
    }

    fn find_list_by_name(&self, name: &str) -> Option<Retained<EKCalendar>> {
        let cals = unsafe { self.store.calendarsForEntityType(EKEntityType::Reminder) };
        let count = cals.count();
        for i in 0..count {
            let cal = cals.objectAtIndex(i);
            if unsafe { cal.title() }.to_string() == name {
                return Some(cal);
            }
        }
        None
    }
}

impl RemindersBackend for EkReminders {
    fn list_reminders(&self, include_completed: bool) -> Result<Vec<ReminderItem>, AppleError> {
        use block2::RcBlock;
        use objc2_foundation::NSArray;
        use std::sync::mpsc;

        let predicate = unsafe { self.store.predicateForRemindersInCalendars(None) };

        let (tx, rx) = mpsc::channel::<Vec<ReminderItem>>();
        let want_completed = include_completed;
        let block = RcBlock::new(move |arr: *mut NSArray<EKReminder>| {
            let mut out = Vec::new();
            if !arr.is_null() {
                let arr: &NSArray<EKReminder> = unsafe { &*arr };
                let count = arr.count();
                for i in 0..count {
                    let r = arr.objectAtIndex(i);
                    if !want_completed && unsafe { r.isCompleted() } {
                        continue;
                    }
                    out.push(EkReminders::to_item(&r));
                }
            }
            let _ = tx.send(out);
        });

        let _token = unsafe {
            self.store
                .fetchRemindersMatchingPredicate_completion(&predicate, &block)
        };

        rx.recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| AppleError::Backend("查询提醒事项超时".into()))
    }

    fn get_reminder(&self, id: &str) -> Result<ReminderItem, AppleError> {
        let r = self.locate(id)?;
        Ok(Self::to_item(&r))
    }

    fn create_reminder(&self, draft: ReminderDraft) -> Result<ReminderItem, AppleError> {
        let r = unsafe { EKReminder::reminderWithEventStore(&self.store) };
        unsafe {
            r.setTitle(Some(&Self::ns(&draft.title)));
            if let Some(notes) = &draft.notes {
                r.setNotes(Some(&Self::ns(notes)));
            }
            if let Some(due) = &draft.due {
                let comps = Self::iso_to_components(due)?;
                r.setDueDateComponents(Some(&comps));
            }
        }
        let cal = match &draft.list {
            Some(name) => self
                .find_list_by_name(name)
                .ok_or_else(|| AppleError::NotFound(format!("提醒列表：{name}")))?,
            None => unsafe { self.store.defaultCalendarForNewReminders() }
                .ok_or_else(|| AppleError::Backend("无可用默认提醒列表".into()))?,
        };
        unsafe { r.setCalendar(Some(&cal)) };

        unsafe { self.store.saveReminder_commit_error(&r, true) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(Self::to_item(&r))
    }

    fn update_reminder(&self, id: &str, patch: ReminderPatch) -> Result<ReminderItem, AppleError> {
        let r = self.locate(id)?;
        unsafe {
            if let Some(t) = &patch.title {
                r.setTitle(Some(&Self::ns(t)));
            }
            if let Some(notes) = &patch.notes {
                r.setNotes(Some(&Self::ns(notes)));
            }
            if let Some(due) = &patch.due {
                let comps = Self::iso_to_components(due)?;
                r.setDueDateComponents(Some(&comps));
            }
        }
        unsafe { self.store.saveReminder_commit_error(&r, true) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(Self::to_item(&r))
    }

    fn complete_reminder(&self, id: &str) -> Result<ReminderItem, AppleError> {
        let r = self.locate(id)?;
        unsafe {
            r.setCompleted(true);
            r.setCompletionDate(Some(&NSDate::now()));
        }
        unsafe { self.store.saveReminder_commit_error(&r, true) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(Self::to_item(&r))
    }

    fn delete_reminder(&self, id: &str) -> Result<(), AppleError> {
        let r = self.locate(id)?;
        unsafe { self.store.removeReminder_commit_error(&r, true) }
            .map_err(|e| Self::map_err(&e))?;
        Ok(())
    }
}

// ===================== 内存 mock =====================

use std::sync::Mutex;

pub struct MockReminders {
    inner: Mutex<MockInner>,
}

struct MockInner {
    items: Vec<ReminderItem>,
    next: u64,
}

impl Default for MockReminders {
    fn default() -> Self {
        Self::new()
    }
}

impl MockReminders {
    pub fn new() -> Self {
        Self { inner: Mutex::new(MockInner { items: Vec::new(), next: 1 }) }
    }
}

impl RemindersBackend for MockReminders {
    fn list_reminders(&self, include_completed: bool) -> Result<Vec<ReminderItem>, AppleError> {
        let g = self.inner.lock().unwrap();
        Ok(g.items
            .iter()
            .filter(|r| include_completed || !r.completed)
            .cloned()
            .collect())
    }

    fn get_reminder(&self, id: &str) -> Result<ReminderItem, AppleError> {
        let g = self.inner.lock().unwrap();
        g.items
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| AppleError::NotFound(id.to_string()))
    }

    fn create_reminder(&self, draft: ReminderDraft) -> Result<ReminderItem, AppleError> {
        if let Some(due) = &draft.due {
            iso_to_unix(due)?;
        }
        let mut g = self.inner.lock().unwrap();
        let id = format!("mock-{}", g.next);
        g.next += 1;
        let item = ReminderItem {
            id: id.clone(),
            title: draft.title,
            due: draft.due,
            completed: false,
            list: draft.list.unwrap_or_else(|| "默认".into()),
            notes: draft.notes,
        };
        g.items.push(item.clone());
        Ok(item)
    }

    fn update_reminder(&self, id: &str, patch: ReminderPatch) -> Result<ReminderItem, AppleError> {
        let mut g = self.inner.lock().unwrap();
        let r = g
            .items
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| AppleError::NotFound(id.to_string()))?;
        if let Some(t) = patch.title {
            r.title = t;
        }
        if let Some(notes) = patch.notes {
            r.notes = Some(notes);
        }
        if let Some(due) = patch.due {
            iso_to_unix(&due)?;
            r.due = Some(due);
        }
        Ok(r.clone())
    }

    fn complete_reminder(&self, id: &str) -> Result<ReminderItem, AppleError> {
        let mut g = self.inner.lock().unwrap();
        let r = g
            .items
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| AppleError::NotFound(id.to_string()))?;
        r.completed = true;
        Ok(r.clone())
    }

    fn delete_reminder(&self, id: &str) -> Result<(), AppleError> {
        let mut g = self.inner.lock().unwrap();
        if !g.items.iter().any(|r| r.id == id) {
            return Err(AppleError::NotFound(id.to_string()));
        }
        g.items.retain(|r| r.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ReminderDraft {
        ReminderDraft {
            title: "买牛奶".into(),
            due: Some("2026-06-28T18:00:00Z".into()),
            notes: Some("两盒".into()),
            list: None,
        }
    }

    #[test]
    fn mock_crud_roundtrip() {
        let rem = MockReminders::new();
        let created = rem.create_reminder(draft()).unwrap();
        assert_eq!(created.title, "买牛奶");
        assert!(!created.completed);
        assert_eq!(created.due.as_deref(), Some("2026-06-28T18:00:00Z"));

        let got = rem.get_reminder(&created.id).unwrap();
        assert_eq!(got, created);

        let updated = rem
            .update_reminder(
                &created.id,
                ReminderPatch { title: Some("买酸奶".into()), ..Default::default() },
            )
            .unwrap();
        assert_eq!(updated.title, "买酸奶");

        // 默认列表只含未完成。
        assert_eq!(rem.list_reminders(false).unwrap().len(), 1);
        let completed = rem.complete_reminder(&created.id).unwrap();
        assert!(completed.completed);
        assert!(rem.list_reminders(false).unwrap().is_empty());
        assert_eq!(rem.list_reminders(true).unwrap().len(), 1);

        rem.delete_reminder(&created.id).unwrap();
        assert!(matches!(rem.get_reminder(&created.id), Err(AppleError::NotFound(_))));
    }

    #[test]
    fn mock_get_missing_not_found() {
        let rem = MockReminders::new();
        assert!(matches!(rem.get_reminder("nope"), Err(AppleError::NotFound(_))));
        assert!(matches!(rem.complete_reminder("nope"), Err(AppleError::NotFound(_))));
        assert!(matches!(rem.delete_reminder("nope"), Err(AppleError::NotFound(_))));
        assert!(matches!(
            rem.update_reminder("nope", ReminderPatch::default()),
            Err(AppleError::NotFound(_))
        ));
    }
}
