//! "Atividade do navegador": the desktop's audit of this Browser — what the
//! interface and the agents did, with the grant and how it ended — newest
//! first, the last 50. It opens from the agent's menu, or (no conversation)
//! from its own clock button; each opening reads it again, and its refresh
//! button too. An answer for an earlier opening is dropped.

use gpui::{AnyElement, Context, FontWeight, IntoElement, Task, div, prelude::*, px};

use super::surface::BrowserSurface;
use crate::data::browser_bridge::AuditRecord;
use crate::i18n::{Args, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// `readBrowserAudit({ limit: 50 })`
const LIMIT: usize = 50;

#[derive(Default)]
pub struct AuditPanel {
    pub open: bool,
    pub loading: bool,
    pub failed: bool,
    pub records: Option<Vec<AuditRecord>>,
    request: u64,
    _read: Option<Task<()>>,
}

impl BrowserSurface {
    /// `handleAuditOpenChange`
    pub fn set_audit_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.audit.request += 1;
            self.audit.open = false;
            self.audit.loading = false;
            self.audit._read = None;
            cx.notify();
            return;
        }
        let Some(token) = self.bridge.as_ref().and_then(|bridge| bridge.token) else {
            return;
        };
        self.audit.open = true;
        self.audit.records = None;
        self.load_audit(token, cx);
    }

    /// `handleAuditRefresh`
    pub fn refresh_audit(&mut self, cx: &mut Context<Self>) {
        let token = self.bridge.as_ref().and_then(|bridge| bridge.token);
        if let Some(token) = token.filter(|_| self.audit.open && !self.audit.loading) {
            self.load_audit(token, cx);
        }
    }

    fn load_audit(&mut self, token: u64, cx: &mut Context<Self>) {
        let Some(api) = self.bridge_api.clone() else {
            return;
        };
        self.audit.request += 1;
        let request_id = self.audit.request;
        self.audit.loading = true;
        self.audit.failed = false;
        cx.notify();
        let request = self.request.clone();
        self.audit._read = Some(cx.spawn(async move |this, cx| {
            let records = cx
                .background_executor()
                .spawn(async move {
                    api.browser_audit(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        token,
                        LIMIT,
                    )
                })
                .await;
            let _ = this.update(cx, |surface, cx| {
                if surface.audit.request != request_id || !surface.audit.open {
                    return;
                }
                surface.audit.loading = false;
                match records {
                    Ok(records) => surface.audit.records = Some(records),
                    Err(_) => surface.audit.failed = true,
                }
                cx.notify();
            });
        }));
    }

    fn audit_entry(
        &self,
        index: usize,
        record: &AuditRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let time = crate::data::time::local_clock(record.timestamp_ms, english)
            .map(Into::into)
            .unwrap_or_else(|| cx.t("browser.audit.unknownTime"));
        let origin = if record.origin == "mcp" {
            let provider = record
                .provider_id
                .clone()
                .unwrap_or_else(|| cx.t("browser.audit.origin.providerUnavailable").to_string());
            cx.t_with(
                "browser.audit.origin.mcp",
                &Args::default().with("provider", provider),
            )
        } else {
            cx.t("browser.audit.origin.ui")
        };
        div()
            .debug_selector(move || format!("browser-audit-entry-{index}"))
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(8.0))
            .py(px(6.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t(&format!("browser.audit.tools.{}", record.tool))),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .text_color(colors.muted_foreground)
                            .child(time),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(6.0))
                    .gap_y(px(2.0))
                    .text_size(px(10.0))
                    .text_color(colors.muted_foreground)
                    .child(origin)
                    .child(cx.t(&format!("browser.audit.grant.{}", record.grant_state)))
                    .child(cx.t(&format!("browser.audit.outcome.{}", record.outcome))),
            )
            .into_any_element()
    }

    /// The card: title, refresh, then loading, the error, the list or
    /// "Ainda não há atividade".
    fn audit_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let loading = self.audit.loading;
        let refresh_label = cx.t("browser.audit.refresh");
        let refresh = Button::new("browser-audit-refresh")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconXs)
            .disabled(loading)
            .on_click(cx.listener(|surface, _, _, cx| surface.refresh_audit(cx)))
            .child(if loading {
                spinner("browser-audit-refreshing", 14.0).into_any_element()
            } else {
                Icon::new("refresh-cw").size(14.0).into_any_element()
            });
        let state = |text: gpui::SharedString| {
            div()
                .py(px(16.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .child(text)
        };
        let body = if loading {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .py(px(16.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(colors.muted_foreground)
                .child(spinner("browser-audit-loading", 12.0))
                .child(cx.t("browser.audit.loading"))
                .into_any_element()
        } else if self.audit.failed {
            state(cx.t("browser.audit.error"))
                .text_color(colors.destructive)
                .into_any_element()
        } else if let Some(records) = self.audit.records.as_ref().filter(|r| !r.is_empty()) {
            let entries: Vec<AnyElement> = records
                .iter()
                .enumerate()
                .map(|(index, record)| self.audit_entry(index, record, cx))
                .collect();
            div()
                .id("browser-audit-list")
                .debug_selector(|| "browser-audit-list".into())
                .flex()
                .flex_col()
                .gap(px(4.0))
                .max_h(px(288.0))
                .overflow_y_scroll()
                .children(entries)
                .into_any_element()
        } else {
            state(cx.t("browser.audit.empty"))
                .text_color(colors.muted_foreground)
                .into_any_element()
        };
        div()
            .debug_selector(|| "browser-audit".into())
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(cx.t("browser.audit.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("browser.audit.description")),
                            ),
                    )
                    .child(Tooltip::new(
                        "browser-audit-refresh-tip",
                        refresh,
                        move |_, _| div().child(refresh_label.clone()).into_any_element(),
                    )),
            )
            .child(body)
            .into_any_element()
    }

    /// The card under `trigger`, end-aligned, `w-80 p-3`.
    pub(super) fn audit_popover(
        &self,
        trigger: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Popover {
        let content = self.audit.open.then(|| self.audit_content(cx));
        let view = cx.entity().downgrade();
        Popover::new(
            "browser-audit-popover",
            self.audit.open,
            trigger,
            move |_, _| content.unwrap_or_else(|| div().into_any_element()),
            move |open, _, cx| {
                let _ = view.update(cx, |surface, cx| surface.set_audit_open(open, cx));
            },
        )
        .align(PopoverAlign::End)
        .width(320.0)
        .padding(12.0)
    }

    /// No conversation, no agent: only the activity's clock button.
    pub(super) fn audit_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.request.session_id.is_some() {
            return None;
        }
        let label = cx.t("browser.audit.open");
        let ready = self
            .bridge
            .as_ref()
            .and_then(|bridge| bridge.token)
            .is_some();
        let button = Button::new("browser-audit-open")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .disabled(!ready)
            .child(Icon::new("history").size(14.0));
        let trigger = Tooltip::new("browser-audit-open-tip", button, move |_, _| {
            div().child(label.clone()).into_any_element()
        });
        Some(self.audit_popover(trigger, cx).into_any_element())
    }
}
