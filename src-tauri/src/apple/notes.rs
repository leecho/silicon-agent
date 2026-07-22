//! 备忘录后端：trait + osascript 真实实现（OsaNotes）+ 内存 mock（MockNotes）。
//! 备忘录无公开框架，CRUD 全走 AppleScript（自动化权限）。输出用控制符分隔避免换行歧义。

use std::sync::Mutex;

use super::osascript::run_osascript;
use super::AppleError;

/// 记录分隔符（RS）/ 字段分隔符（US），避开正文里的换行与逗号。
const RS: char = '\u{1e}';
const US: char = '\u{1f}';
const TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NoteItem {
    pub id: String,
    pub title: String,
    pub body: String,
    pub folder: String,
}

#[derive(Debug, Clone)]
pub struct NoteDraft {
    pub title: String,
    pub body: String,
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NotePatch {
    pub title: Option<String>,
    pub body: Option<String>,
}

pub trait NotesBackend: Send + Sync {
    fn list_notes(&self, folder: Option<&str>) -> Result<Vec<NoteItem>, AppleError>;
    fn get_note(&self, id: &str) -> Result<NoteItem, AppleError>;
    fn create_note(&self, draft: NoteDraft) -> Result<NoteItem, AppleError>;
    fn update_note(&self, id: &str, patch: NotePatch) -> Result<NoteItem, AppleError>;
    fn delete_note(&self, id: &str) -> Result<(), AppleError>;
}

/// 转义 AppleScript 字符串字面量中的反斜杠与双引号。
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 解析 list 脚本输出：记录用 RS、字段用 US（id/title/body/folder）。
fn parse_notes(out: &str) -> Vec<NoteItem> {
    out.split(RS)
        .filter(|r| !r.trim().is_empty())
        .filter_map(|rec| {
            let mut f = rec.split(US);
            let id = f.next()?.to_string();
            let title = f.next().unwrap_or("").to_string();
            let body = f.next().unwrap_or("").to_string();
            let folder = f.next().unwrap_or("").to_string();
            Some(NoteItem { id, title, body, folder })
        })
        .collect()
}

/// osascript 真实备忘录后端。
pub struct OsaNotes;

impl OsaNotes {
    pub fn new() -> Self {
        OsaNotes
    }
}

impl Default for OsaNotes {
    fn default() -> Self {
        Self::new()
    }
}

impl NotesBackend for OsaNotes {
    fn list_notes(&self, folder: Option<&str>) -> Result<Vec<NoteItem>, AppleError> {
        let source = match folder {
            Some(name) => format!("notes of folder \"{}\"", esc(name)),
            None => "notes".to_string(),
        };
        let script = format!(
            "set RS to (ASCII character 30)\n\
             set US to (ASCII character 31)\n\
             set out to \"\"\n\
             tell application \"Notes\"\n\
             repeat with n in ({source})\n\
             set out to out & (id of n) & US & (name of n) & US & (plaintext of n) & US & (name of container of n) & RS\n\
             end repeat\n\
             end tell\n\
             return out"
        );
        Ok(parse_notes(&run_osascript(&script, TIMEOUT_MS)?))
    }

    fn get_note(&self, id: &str) -> Result<NoteItem, AppleError> {
        let script = format!(
            "set US to (ASCII character 31)\n\
             tell application \"Notes\"\n\
             set n to note id \"{id}\"\n\
             return (id of n) & US & (name of n) & US & (plaintext of n) & US & (name of container of n)\n\
             end tell",
            id = esc(id)
        );
        let out = run_osascript(&script, TIMEOUT_MS)?;
        parse_single(&out).ok_or_else(|| AppleError::NotFound(id.to_string()))
    }

    fn create_note(&self, draft: NoteDraft) -> Result<NoteItem, AppleError> {
        // Notes 的 note body 为 HTML；首行作为标题。用 title + 换行 + body 组合。
        let html_body = format!("{}<br>{}", esc(&draft.title), esc(&draft.body));
        let target = match &draft.folder {
            Some(name) => format!("at folder \"{}\"", esc(name)),
            None => String::new(),
        };
        let script = format!(
            "set US to (ASCII character 31)\n\
             tell application \"Notes\"\n\
             set n to make new note {target} with properties {{body:\"{body}\"}}\n\
             return (id of n) & US & (name of n) & US & (plaintext of n) & US & (name of container of n)\n\
             end tell",
            target = target,
            body = html_body
        );
        let out = run_osascript(&script, TIMEOUT_MS)?;
        parse_single(&out).ok_or_else(|| AppleError::Backend("创建后未能读回备忘录".into()))
    }

    fn update_note(&self, id: &str, patch: NotePatch) -> Result<NoteItem, AppleError> {
        let mut sets = String::new();
        if let Some(t) = &patch.title {
            sets.push_str(&format!("set name of n to \"{}\"\n", esc(t)));
        }
        if let Some(b) = &patch.body {
            sets.push_str(&format!("set body of n to \"{}\"\n", esc(b)));
        }
        let script = format!(
            "set US to (ASCII character 31)\n\
             tell application \"Notes\"\n\
             set n to note id \"{id}\"\n\
             {sets}\
             return (id of n) & US & (name of n) & US & (plaintext of n) & US & (name of container of n)\n\
             end tell",
            id = esc(id),
            sets = sets
        );
        let out = run_osascript(&script, TIMEOUT_MS)?;
        parse_single(&out).ok_or_else(|| AppleError::NotFound(id.to_string()))
    }

    fn delete_note(&self, id: &str) -> Result<(), AppleError> {
        let script = format!(
            "tell application \"Notes\"\n\
             delete note id \"{id}\"\n\
             end tell",
            id = esc(id)
        );
        run_osascript(&script, TIMEOUT_MS).map(|_| ())
    }
}

/// 解析单条 get/create 输出（无 RS，仅 4 个 US 字段）。
fn parse_single(out: &str) -> Option<NoteItem> {
    let mut f = out.split(US);
    let id = f.next()?.to_string();
    if id.trim().is_empty() {
        return None;
    }
    let title = f.next().unwrap_or("").to_string();
    let body = f.next().unwrap_or("").to_string();
    let folder = f.next().unwrap_or("").to_string();
    Some(NoteItem { id, title, body, folder })
}

/// 内存 mock：供工具层与非 mac 逻辑测试。
pub struct MockNotes {
    items: Mutex<Vec<NoteItem>>,
    seq: Mutex<u64>,
}

impl MockNotes {
    pub fn new() -> Self {
        MockNotes {
            items: Mutex::new(Vec::new()),
            seq: Mutex::new(0),
        }
    }
}

impl Default for MockNotes {
    fn default() -> Self {
        Self::new()
    }
}

impl NotesBackend for MockNotes {
    fn list_notes(&self, folder: Option<&str>) -> Result<Vec<NoteItem>, AppleError> {
        let items = self.items.lock().unwrap();
        Ok(items
            .iter()
            .filter(|n| folder.map_or(true, |f| n.folder == f))
            .cloned()
            .collect())
    }

    fn get_note(&self, id: &str) -> Result<NoteItem, AppleError> {
        let items = self.items.lock().unwrap();
        items
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .ok_or_else(|| AppleError::NotFound(id.to_string()))
    }

    fn create_note(&self, draft: NoteDraft) -> Result<NoteItem, AppleError> {
        let mut seq = self.seq.lock().unwrap();
        *seq += 1;
        let item = NoteItem {
            id: format!("mock-note-{}", *seq),
            title: draft.title,
            body: draft.body,
            folder: draft.folder.unwrap_or_else(|| "Notes".to_string()),
        };
        self.items.lock().unwrap().push(item.clone());
        Ok(item)
    }

    fn update_note(&self, id: &str, patch: NotePatch) -> Result<NoteItem, AppleError> {
        let mut items = self.items.lock().unwrap();
        let n = items
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| AppleError::NotFound(id.to_string()))?;
        if let Some(t) = patch.title {
            n.title = t;
        }
        if let Some(b) = patch.body {
            n.body = b;
        }
        Ok(n.clone())
    }

    fn delete_note(&self, id: &str) -> Result<(), AppleError> {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|n| n.id != id);
        if items.len() == before {
            return Err(AppleError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_notes_splits_records_and_fields() {
        let out = format!("id1{US}标题1{US}正文1{US}文件夹{RS}id2{US}标题2{US}正文2{US}文件夹{RS}");
        let v = parse_notes(&out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "id1");
        assert_eq!(v[0].title, "标题1");
        assert_eq!(v[1].folder, "文件夹");
    }

    #[test]
    fn esc_escapes_quotes_and_backslash() {
        assert_eq!(esc("a\"b\\c"), "a\\\"b\\\\c");
    }

    #[test]
    fn mock_crud_roundtrip() {
        let b = MockNotes::new();
        let n = b
            .create_note(NoteDraft {
                title: "待办".into(),
                body: "买牛奶".into(),
                folder: None,
            })
            .unwrap();
        assert_eq!(b.get_note(&n.id).unwrap().title, "待办");
        let upd = b
            .update_note(
                &n.id,
                NotePatch {
                    title: Some("已办".into()),
                    body: None,
                },
            )
            .unwrap();
        assert_eq!(upd.title, "已办");
        assert_eq!(b.list_notes(None).unwrap().len(), 1);
        b.delete_note(&n.id).unwrap();
        assert!(b.get_note(&n.id).is_err());
    }

    #[test]
    fn mock_get_missing_not_found() {
        let b = MockNotes::new();
        assert_eq!(b.get_note("nope"), Err(AppleError::NotFound("nope".into())));
    }
}
