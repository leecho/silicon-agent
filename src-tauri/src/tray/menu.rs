const MAX_MENU_TITLE_CHARS: usize = 28;
const PRIMARY_SESSION_LIMIT: usize = 5;
const MORE_SESSION_LIMIT: usize = 20;

pub const TRAY_NEW_TASK_ID: &str = "tray:new-task";
pub const TRAY_SHOW_ID: &str = "show-main-window";
pub const TRAY_QUIT_ID: &str = "quit-app";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayEntity {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayEntitySplit {
    pub primary: Vec<TrayEntity>,
    pub more: Vec<TrayEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuSnapshot {
    pub sessions: TrayEntitySplit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySession {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub is_draft: bool,
    pub origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    NewTask,
    ShowMainWindow,
    Quit,
    OpenSession(String),
}

pub fn session_item_id(id: &str) -> String {
    format!("tray:open-session:{id}")
}

pub fn truncate_menu_title(title: &str) -> String {
    let count = title.chars().count();
    if count <= MAX_MENU_TITLE_CHARS {
        return title.to_string();
    }
    title
        .chars()
        .take(MAX_MENU_TITLE_CHARS.saturating_sub(3))
        .collect::<String>()
        + "..."
}

pub fn split_entities(entities: Vec<TrayEntity>) -> TrayEntitySplit {
    let primary = entities
        .iter()
        .take(PRIMARY_SESSION_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    let more = entities
        .into_iter()
        .skip(PRIMARY_SESSION_LIMIT)
        .take(MORE_SESSION_LIMIT)
        .collect::<Vec<_>>();

    TrayEntitySplit { primary, more }
}

pub fn split_recent_sessions(mut sessions: Vec<TraySession>) -> TrayEntitySplit {
    sessions.retain(|session| !session.is_draft && session.origin == "user");
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    split_entities(
        sessions
            .into_iter()
            .map(|session| TrayEntity {
                id: session.id,
                title: session.title,
            })
            .collect::<Vec<_>>(),
    )
}

pub fn parse_tray_item_id(id: &str) -> Option<TrayAction> {
    if id == TRAY_NEW_TASK_ID {
        return Some(TrayAction::NewTask);
    }
    if id == TRAY_SHOW_ID {
        return Some(TrayAction::ShowMainWindow);
    }
    if id == TRAY_QUIT_ID {
        return Some(TrayAction::Quit);
    }
    if let Some(rest) = id.strip_prefix("tray:open-session:") {
        return Some(TrayAction::OpenSession(rest.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, title: &str, updated_at: &str) -> TraySession {
        TraySession {
            id: id.to_string(),
            title: title.to_string(),
            updated_at: updated_at.to_string(),
            is_draft: false,
            origin: "user".to_string(),
        }
    }

    #[test]
    fn splits_first_five_sessions_and_caps_more_at_twenty() {
        let sessions = (0..30)
            .map(|i| {
                session(
                    &format!("s-{i:02}"),
                    &format!("Session {i:02}"),
                    &format!("{i:02}"),
                )
            })
            .collect::<Vec<_>>();

        let split = split_recent_sessions(sessions);

        assert_eq!(split.primary.len(), 5);
        assert_eq!(split.primary[0].id, "s-29");
        assert_eq!(split.primary[4].id, "s-25");
        assert_eq!(split.more.len(), 20);
        assert_eq!(split.more[0].id, "s-24");
        assert_eq!(split.more[19].id, "s-05");
    }

    #[test]
    fn filters_non_user_and_draft_sessions() {
        let mut draft = session("draft", "Draft", "30");
        draft.is_draft = true;
        let mut scheduled = session("scheduled", "Scheduled", "20");
        scheduled.origin = "scheduled".to_string();

        let split = split_recent_sessions(vec![draft, scheduled, session("user", "User", "10")]);

        assert_eq!(split.primary.len(), 1);
        assert_eq!(split.primary[0].id, "user");
        assert!(split.more.is_empty());
    }

    #[test]
    fn truncates_long_titles_without_touching_short_titles() {
        assert_eq!(truncate_menu_title("短标题"), "短标题");
        assert_eq!(
            truncate_menu_title("abcdefghijklmnopqrstuvwxyz1234567890"),
            "abcdefghijklmnopqrstuvwxy..."
        );
    }

    #[test]
    fn parses_open_item_ids_and_rejects_unknown_ids() {
        assert_eq!(
            parse_tray_item_id("tray:open-session:session-1"),
            Some(TrayAction::OpenSession("session-1".to_string())),
        );
        assert_eq!(
            parse_tray_item_id("show-main-window"),
            Some(TrayAction::ShowMainWindow)
        );
        assert_eq!(parse_tray_item_id("quit-app"), Some(TrayAction::Quit));
        assert_eq!(parse_tray_item_id("tray:unknown:1"), None);
    }

    #[test]
    fn splits_generic_entities_into_first_five_and_more() {
        let entities = (0..7)
            .map(|i| TrayEntity {
                id: format!("item-{i}"),
                title: format!("Item {i}"),
            })
            .collect::<Vec<_>>();

        let split = split_entities(entities);

        assert_eq!(split.primary.len(), 5);
        assert_eq!(split.primary[0].id, "item-0");
        assert_eq!(split.primary[4].id, "item-4");
        assert_eq!(split.more.len(), 2);
        assert_eq!(split.more[0].id, "item-5");
    }
}
