//! Text from the desktop's own catalogs (`apps/desktop/src/i18n/locales`),
//! read with i18next's rules: dotted keys, `{{name}}` interpolation and
//! `_one`/`_other` plurals. One catalog for both apps, no copied strings.

use std::collections::HashMap;

use gpui::{App, Global, SharedString};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PT_BR: &str = include_str!("../../../desktop/src/i18n/locales/pt-BR/common.json");
const EN: &str = include_str!("../../../desktop/src/i18n/locales/en/common.json");

/// The desktop's `AppLocale`; Portuguese is its default too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "en")]
    En,
}

impl Locale {
    /// i18next's plural category: Portuguese counts 0 and 1 as "one".
    fn plural_suffix(self, count: i64) -> &'static str {
        let one = match self {
            Locale::PtBr => count == 0 || count == 1,
            Locale::En => count == 1,
        };
        if one { "_one" } else { "_other" }
    }
}

pub struct I18n {
    locale: Locale,
    catalog: Value,
    fallback: Value,
}

impl Global for I18n {}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let parse = |source: &str| serde_json::from_str(source).unwrap_or(Value::Null);
        let (catalog, fallback) = match locale {
            Locale::PtBr => (parse(PT_BR), parse(EN)),
            Locale::En => (parse(EN), Value::Null),
        };
        Self {
            locale,
            catalog,
            fallback,
        }
    }

    pub fn init(locale: Locale, cx: &mut App) {
        cx.set_global(Self::new(locale));
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// `t("sidebar.tasksTitle")`; a missing key shows the key, like i18next.
    pub fn t(&self, key: &str) -> SharedString {
        self.t_with(key, &Args::default())
    }

    pub fn t_with(&self, key: &str, args: &Args) -> SharedString {
        // i18next reads `_zero` for a count of 0 when the catalog has one.
        let template = args
            .count
            .filter(|count| *count == 0)
            .and_then(|_| self.lookup(&format!("{key}_zero")))
            .or_else(|| {
                args.count.and_then(|count| {
                    self.lookup(&format!("{key}{}", self.locale.plural_suffix(count)))
                })
            })
            .or_else(|| self.lookup(key));
        match template {
            Some(template) => interpolate(template, args).into(),
            None => SharedString::from(key.to_string()),
        }
    }

    /// Whether `key` exists (with its plural form for `args.count`), for
    /// i18next's `defaultValue` fallback.
    pub fn has(&self, key: &str, args: &Args) -> bool {
        args.count
            .and_then(|count| self.lookup(&format!("{key}{}", self.locale.plural_suffix(count))))
            .or_else(|| self.lookup(key))
            .is_some()
    }

    /// `t(key, { returnObjects: true })` on an array of strings; nothing
    /// when the key is missing or is not a list.
    pub fn t_list(&self, key: &str) -> Vec<SharedString> {
        let list = |catalog: &Value| {
            key.split('.')
                .try_fold(catalog, |node, part| node.get(part))
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|item| SharedString::from(item.to_string()))
                        .collect::<Vec<_>>()
                })
        };
        list(&self.catalog)
            .or_else(|| list(&self.fallback))
            .unwrap_or_default()
    }

    fn lookup(&self, key: &str) -> Option<&str> {
        find(&self.catalog, key).or_else(|| find(&self.fallback, key))
    }
}

/// Values for `{{name}}`; `count` also picks the plural form.
#[derive(Clone, Debug, Default)]
pub struct Args {
    count: Option<i64>,
    values: HashMap<&'static str, String>,
}

impl Args {
    pub fn count(count: i64) -> Self {
        Self::default().with_count(count)
    }

    pub fn with_count(mut self, count: i64) -> Self {
        self.count = Some(count);
        self.values.insert("count", count.to_string());
        self
    }

    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.values.insert(name, value.into());
        self
    }
}

fn find<'a>(catalog: &'a Value, key: &str) -> Option<&'a str> {
    key.split('.')
        .try_fold(catalog, |node, part| node.get(part))
        .and_then(Value::as_str)
}

fn interpolate(template: &str, args: &Args) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        if let Some(value) = args.values.get(name) {
            out.push_str(value);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

pub trait Translate {
    fn t(&self, key: &str) -> SharedString;
    fn t_with(&self, key: &str, args: &Args) -> SharedString;
    fn t_list(&self, key: &str) -> Vec<SharedString>;
}

impl Translate for App {
    fn t(&self, key: &str) -> SharedString {
        self.global::<I18n>().t(key)
    }

    fn t_with(&self, key: &str, args: &Args) -> SharedString {
        self.global::<I18n>().t_with(key, args)
    }

    fn t_list(&self, key: &str) -> Vec<SharedString> {
        self.global::<I18n>().t_list(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_desktop_catalog() {
        let pt = I18n::new(Locale::PtBr);
        assert_eq!(pt.t("sidebar.tasksTitle").as_ref(), "Tarefas");
        assert_eq!(
            I18n::new(Locale::En).t("sidebar.tasksTitle").as_ref(),
            "Tasks"
        );
    }

    #[test]
    fn zero_has_its_own_form_when_the_catalog_has_one() {
        let pt = I18n::new(Locale::PtBr);
        let none = pt.t_with("settings.navigation.results", &Args::count(0));
        let two = pt.t_with("settings.navigation.results", &Args::count(2));
        assert_ne!(none, two);
        assert!(two.contains('2'), "{two}");
    }

    #[test]
    fn lists_come_back_whole() {
        let pt = I18n::new(Locale::PtBr);
        let steps = pt.t_list("help.topics.workspaces.steps");
        assert!(steps.len() > 1, "{steps:?}");
        assert!(pt.t_list("help.topics.workspaces.label").is_empty());
        assert!(pt.t_list("help.topics.missing.steps").is_empty());
    }

    #[test]
    fn plurals_follow_each_language() {
        let pt = I18n::new(Locale::PtBr);
        let en = I18n::new(Locale::En);
        let key = "sidebar.attention.label";
        assert_eq!(
            pt.t_with(key, &Args::count(1)).as_ref(),
            "1 tarefa precisa de você"
        );
        assert_eq!(
            pt.t_with(key, &Args::count(3)).as_ref(),
            "3 tarefas precisam de você"
        );
        assert_eq!(en.t_with(key, &Args::count(0)).as_ref(), "0 tasks need you");
    }

    #[test]
    fn a_key_without_plural_forms_still_takes_count() {
        let pt = I18n::new(Locale::PtBr);
        assert_eq!(
            pt.t_with("sidebar.runningCount", &Args::count(2)).as_ref(),
            "2 rodando"
        );
    }

    #[test]
    fn interpolates_named_values() {
        let pt = I18n::new(Locale::PtBr);
        let text = pt.t_with(
            "sidebar.activityTime.ago",
            &Args::default().with("time", "5min"),
        );
        assert_eq!(text.as_ref(), "há 5min");
    }

    #[test]
    fn missing_keys_show_the_key() {
        assert_eq!(
            I18n::new(Locale::En).t("nope.missing").as_ref(),
            "nope.missing"
        );
    }
}
