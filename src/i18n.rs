//! Explicit UI messages. User input, paths and publisher text are never translated.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Deref, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Russian,
    French,
    German,
    Spanish,
    Italian,
    #[default]
    #[serde(other)]
    English,
}

pub const LANGUAGES: [Language; 6] = [
    Language::English,
    Language::Russian,
    Language::French,
    Language::German,
    Language::Spanish,
    Language::Italian,
];

static CATALOG: LazyLock<BTreeMap<String, [String; 5]>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../locales/messages.json"))
        .expect("The embedded translation catalog must be valid")
});

impl Language {
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }

    pub fn text(self, english: &str) -> &str {
        let index = match self {
            Self::English => return english,
            Self::Russian => 0,
            Self::French => 1,
            Self::German => 2,
            Self::Spanish => 3,
            Self::Italian => 4,
        };
        CATALOG
            .get(english)
            .map_or(english, |translations| translations[index].as_str())
    }

    pub fn format(self, english: &str, values: &[String]) -> String {
        // Substitute in a single pass: braces inside a path/name remain literal.
        let template = self.text(english);
        let mut rendered = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            rendered.push_str(&rest[..open]);
            let tail = &rest[open + 1..];
            if let Some(close) = tail.find('}')
                && let Ok(index) = tail[..close].parse::<usize>()
                && let Some(value) = values.get(index)
            {
                rendered.push_str(value);
                rest = &tail[close + 1..];
            } else {
                rendered.push('{');
                rest = tail;
            }
        }
        rendered.push_str(rest);
        rendered
    }
}

/// Keeps message identity so an already visible status changes language immediately.
#[derive(Clone, Debug)]
pub struct Message {
    key: &'static str,
    values: Vec<String>,
    english: String,
}
impl Message {
    pub fn new(key: &'static str, values: Vec<String>) -> Self {
        let english = Language::English.format(key, &values);
        Self {
            key,
            values,
            english,
        }
    }
    pub fn render(&self, language: Language) -> String {
        language.format(self.key, &self.values)
    }
}
impl From<&'static str> for Message {
    fn from(key: &'static str) -> Self {
        Self::new(key, vec![])
    }
}
impl Deref for Message {
    type Target = str;
    fn deref(&self) -> &str {
        &self.english
    }
}
impl PartialEq<&str> for Message {
    fn eq(&self, other: &&str) -> bool {
        self.english == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placeholders(text: &str) -> Vec<String> {
        let mut found = vec![];
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            let close = rest.find('}').expect("closed placeholder");
            found.push(rest[..close].to_string());
            rest = &rest[close + 1..];
        }
        found.sort();
        found
    }
    #[test]
    fn every_catalog_message_has_five_complete_placeholder_preserving_translations() {
        assert!(CATALOG.len() > 100);
        for (key, translations) in CATALOG.iter() {
            for translated in translations {
                assert!(!translated.trim().is_empty(), "{key}");
                assert_eq!(placeholders(key), placeholders(translated), "{key}");
            }
        }
    }
    #[test]
    fn literal_ui_lookups_and_parameterized_messages_are_registered() {
        let source = include_str!("ui.rs");
        for marker in ["l.text(", "l.format(", "Message::new("] {
            let mut rest = source;
            while let Some(offset) = rest.find(marker) {
                rest = &rest[offset + marker.len()..];
                let value = rest.trim_start();
                if !value.starts_with('"') {
                    continue;
                }
                let mut escaped = false;
                let end = value
                    .char_indices()
                    .skip(1)
                    .find_map(|(index, character)| {
                        if escaped {
                            escaped = false;
                            None
                        } else if character == '\\' {
                            escaped = true;
                            None
                        } else if character == '"' {
                            Some(index + 1)
                        } else {
                            None
                        }
                    })
                    .expect("terminated literal");
                let key: String = serde_json::from_str(&value[..end]).unwrap();
                assert!(CATALOG.contains_key(&key), "Untranslated UI message: {key}");
            }
        }
    }
    #[test]
    fn formatting_never_reinterprets_user_values_or_loses_message_identity() {
        let message = Message::new("Original: {0}", vec![r"C:\User {1}\data".into()]);
        assert_eq!(
            message.render(Language::Russian),
            r"Исходный путь: C:\User {1}\data"
        );
        assert_eq!(
            message.render(Language::English),
            r"Original: C:\User {1}\data"
        );
        assert_eq!(
            Language::French.text("unsupported diagnostic"),
            "unsupported diagnostic"
        );
    }
}
