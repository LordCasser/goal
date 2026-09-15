//! Persist one application locale. User data, logs and protocol IDs are never translated.
use crate::{error::AppResult, repository::settings as repo};
use rusqlite::Connection;
use serde_json::Value;

pub const KEY_LOCALE: &str = "locale";

/// A locale-independent piece of user-visible text. The key is stable data;
/// arguments contain values from the user's records and are never translated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocalizedMessage {
    pub key: String,
    #[serde(default)]
    pub args: Value,
}

impl LocalizedMessage {
    pub fn new(key: impl Into<String>, args: Value) -> Self {
        Self {
            key: key.into(),
            args,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl Locale {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "zh-CN" => Some(Self::ZhCn),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
        }
    }
    pub fn from_system(value: &str) -> Self {
        if value
            .split(['-', '_'])
            .next()
            .is_some_and(|v| v.eq_ignore_ascii_case("zh"))
        {
            Self::ZhCn
        } else {
            Self::En
        }
    }
    pub fn instruction(self) -> &'static str {
        match self {
            Self::En => "Default response language: English. Use it for assistant replies and generated explanations unless the user explicitly requests another language. Preserve user-provided task titles and content; never translate existing records just to match the interface.",
            Self::ZhCn => "Default response language: Simplified Chinese (简体中文). Use it for assistant replies and generated explanations unless the user explicitly requests another language. Preserve user-provided task titles and content; never translate existing records just to match the interface.",
        }
    }
}

pub fn current(conn: &Connection) -> AppResult<Locale> {
    let stored = repo::get(conn, KEY_LOCALE)?;
    if let Some(locale) = stored.as_deref().and_then(Locale::parse) {
        return Ok(locale);
    }
    let locale = Locale::from_system(&sys_locale::get_locale().unwrap_or_default());
    repo::set(conn, KEY_LOCALE, locale.as_str())?;
    Ok(locale)
}

pub fn for_db(db: &crate::db::Db) -> AppResult<Locale> {
    current(&*db.pool().get()?)
}

fn action_catalog(locale: Locale) -> &'static std::collections::HashMap<String, String> {
    use std::sync::LazyLock;
    static EN: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../src/lib/i18n/locales/en/backend-actions.json"
        ))
        .expect("valid English backend action catalog")
    });
    static ZH: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../src/lib/i18n/locales/zh-CN/backend-actions.json"
        ))
        .expect("valid Chinese backend action catalog")
    });
    match locale {
        Locale::En => &EN,
        Locale::ZhCn => &ZH,
    }
}

fn action_key(key: &str) -> &str {
    key.strip_prefix("backend-actions:").unwrap_or(key)
}

fn render_value(locale: Locale, value: &Value) -> String {
    if let (Some(key), Some(args)) = (value.get("key").and_then(Value::as_str), value.get("args")) {
        return render_message(locale, &LocalizedMessage::new(key, args.clone()));
    }
    match value {
        Value::Null => "".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| render_value(locale, value))
            .collect::<Vec<_>>()
            .join(if locale == Locale::ZhCn { "；" } else { "; " }),
        Value::Object(_) => value.to_string(),
    }
}

/// Render a structured action message. Missing action keys fall back to the
/// English catalog so old clients never lose the approval context.
pub fn render_message(locale: Locale, message: &LocalizedMessage) -> String {
    use std::sync::LazyLock;
    static EN: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../src/lib/i18n/locales/en/backend-actions.json"
        ))
        .expect("valid English backend action catalog")
    });
    let key = action_key(&message.key);
    let template = action_catalog(locale)
        .get(key)
        .or_else(|| EN.get(key))
        .map(String::as_str)
        .unwrap_or(key);
    let Some(args) = message.args.as_object() else {
        return template.to_string();
    };
    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        output.push_str(&remaining[..start]);
        let tail = &remaining[start + 2..];
        let Some(end) = tail.find("}}") else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let name = &tail[..end];
        if let Some(value) = args.get(name) {
            output.push_str(&render_value(locale, value));
        } else {
            output.push_str(&remaining[start..start + 2 + end + 2]);
        }
        remaining = &tail[end + 2..];
    }
    output.push_str(remaining);
    output
}

pub fn text(locale: Locale, key: &str, args: &[(&str, String)]) -> String {
    use std::sync::LazyLock;
    static EN: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../src/lib/i18n/locales/en/backend.json"))
            .expect("valid English catalog")
    });
    static ZH: LazyLock<std::collections::HashMap<String, String>> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../src/lib/i18n/locales/zh-CN/backend.json"
        ))
        .expect("valid Chinese catalog")
    });
    let catalog = match locale {
        Locale::En => &*EN,
        Locale::ZhCn => &*ZH,
    };
    let mut remaining = catalog
        .get(key)
        .or_else(|| EN.get(key))
        .expect("registered backend message")
        .as_str();
    let mut output = String::new();
    while let Some(start) = remaining.find("{{") {
        output.push_str(&remaining[..start]);
        let tail = &remaining[start + 2..];
        let Some(end) = tail.find("}}") else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let name = &tail[..end];
        if let Some((_, value)) = args.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push_str(&remaining[start..start + 2 + end + 2]);
        }
        remaining = &tail[end + 2..];
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_matching_and_strict_preferences_have_different_boundaries() {
        assert_eq!(Locale::from_system("zh-Hans-CN"), Locale::ZhCn);
        assert_eq!(Locale::from_system("zh_CN.UTF-8"), Locale::ZhCn);
        assert_eq!(Locale::from_system("de-DE"), Locale::En);
        assert_eq!(Locale::parse("zh"), None);
        assert_eq!(Locale::parse("en-US"), None);
    }
    #[test]
    fn language_instruction_preserves_user_content_and_explicit_requests() {
        for locale in [Locale::En, Locale::ZhCn] {
            assert!(locale.instruction().contains("explicitly requests"));
            assert!(locale.instruction().contains("Preserve user-provided"));
        }
        assert_ne!(Locale::En.instruction(), Locale::ZhCn.instruction());
    }
    #[test]
    fn notifications_and_interpolated_user_content_follow_the_locale_boundary() {
        assert_eq!(
            text(Locale::En, "notification.day_one", &[("count", "1".into())]),
            "Today's plan is ready — 1 item scheduled."
        );
        assert_eq!(
            text(
                Locale::ZhCn,
                "notification.day_other",
                &[("count", "2".into())]
            ),
            "今日计划已就绪，安排了 2 项事务。"
        );
        assert!(text(
            Locale::En,
            "issue.goalsDetail",
            &[
                ("titles", "{{count}} 用户内容".into()),
                ("count", "2".into()),
                ("threshold", "3".into())
            ]
        )
        .contains("{{count}} 用户内容"));
    }

    #[test]
    fn structured_messages_render_nested_arguments_in_the_selected_locale() {
        let label = LocalizedMessage::new(
            "backend-actions:settings.label.theme",
            serde_json::json!({}),
        );
        let message = LocalizedMessage::new(
            "backend-actions:settings.changed",
            serde_json::json!({"label": label, "before": "white", "after": "gray"}),
        );
        assert_eq!(render_message(Locale::En, &message), "Theme: white → gray");
        assert_eq!(render_message(Locale::ZhCn, &message), "主题：white → gray");
    }
}
