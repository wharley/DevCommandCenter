//! "Criar PR" from the conversation's menu (`CreateChangeRequestDialog`):
//! the route and where it opens, the title (the task's by default), the
//! description, the local changes committed and pushed first (checked when
//! there are some), draft. It reads the forge and the request's context
//! when it opens; it can only be sent once the forge can act.

use gpui::{
    AnyElement, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, SharedString,
    Task, Window, div, prelude::*, px,
};

use super::super::{CodeColumn, Loaded};
use super::dialogs::field;
use super::dock::dock_failed;
use super::draft::{DraftField, draft_label};
use crate::data::desktop_api::{ChangeRequestContext, RequestInput};
use crate::data::forge::ForgeContext;
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, fonts};
use crate::ui::text_area::TextArea;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Dialog, Toaster, dialog_footer, dialog_header,
};

pub(crate) struct RequestDialog {
    pub draft: bool,
    pub include_local: bool,
    pub title: Entity<TextInput>,
    pub body: Entity<TextArea>,
    /// The local changes when it opened: files, +, −.
    pub local: (u32, u32, u32),
    /// The forge (`forgeContextQuery`), once read; `Some(None)` when it failed.
    pub forge: Option<Option<ForgeContext>>,
    pub context: Option<ChangeRequestContext>,
    /// The title the person has not touched (the default may still arrive).
    default_title: String,
    pub sending: bool,
    _read: Task<()>,
    _title_events: gpui::Subscription,
}

impl RequestDialog {
    fn forge_ready(&self) -> bool {
        matches!(&self.forge, Some(Some(forge)) if forge.ready())
    }
}

impl CodeColumn {
    /// The dialog, draft or not (`setCreateRequestDraft`).
    pub(super) fn open_request_dialog(
        &mut self,
        draft: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let default_title = self
            .delivery
            .task
            .as_ref()
            .and_then(|task| task.name.clone())
            .unwrap_or_default();
        let local = self.dock_summary().map_or((0, 0, 0), |summary| {
            (summary.files, summary.additions, summary.deletions)
        });
        let title = cx.new(|cx| {
            let mut input = TextInput::new(window, cx);
            input.set_text(default_title.clone(), cx);
            input
        });
        let placeholder = cx.t("composer.executionDock.createRequest.descriptionPlaceholder");
        let body = cx.new(|cx| TextArea::new(window, cx).with_placeholder(placeholder));
        title.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        // Enter sends (the form's submit), Esc closes.
        let title_events = cx.subscribe(
            &title,
            |column, _, event: &TextInputEvent, cx| match event {
                TextInputEvent::Submitted => column.send_request(cx),
                TextInputEvent::Cancelled => column.close_request(cx),
                _ => {}
            },
        );
        let api = self.api();
        let read = cx.spawn(async move |this, cx| {
            let (forge, context) = cx
                .background_executor()
                .spawn(async move {
                    let Ok(api) = api else {
                        return (None, None);
                    };
                    let forge = api.forge_context(&root).ok();
                    let login = forge
                        .as_ref()
                        .and_then(|forge| forge.effective_login.clone());
                    let context = api.change_request_context(&root, login.as_deref()).ok();
                    (forge, context)
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                let Some(request) = column.delivery.request.as_mut() else {
                    return;
                };
                request.forge = Some(forge);
                // The default title follows the context, as the desktop's
                // effect does, unless the person already wrote one.
                let untouched = request.title.read(cx).text() == request.default_title;
                let title = context.as_ref().and_then(|context| context.title.clone());
                if let Some(title) = title.filter(|_| untouched) {
                    request.default_title = title.clone();
                    request
                        .title
                        .update(cx, |input, cx| input.set_text(title, cx));
                }
                request.context = context;
                cx.notify();
            });
        });
        self.delivery.request = Some(RequestDialog {
            draft,
            include_local: local.0 > 0,
            title,
            body,
            local,
            forge: None,
            context: None,
            default_title,
            sending: false,
            _read: read,
            _title_events: title_events,
        });
        cx.notify();
    }

    pub(crate) fn close_request(&mut self, cx: &mut Context<Self>) {
        if self
            .delivery
            .request
            .as_ref()
            .is_some_and(|request| !request.sending)
        {
            self.delivery.request = None;
            self.delivery.drafting = None;
            cx.notify();
        }
    }

    pub(crate) fn toggle_request(&mut self, draft: bool, checked: bool, cx: &mut Context<Self>) {
        if let Some(request) = self.delivery.request.as_mut() {
            if draft {
                request.draft = checked;
            } else {
                request.include_local = checked;
            }
            cx.notify();
        }
    }

    /// `createRequest`: the local changes first when asked, then the request.
    pub(crate) fn send_request(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id() else {
            return;
        };
        let label = self.request_label_for_dialog();
        let Some(request) = self.delivery.request.as_mut() else {
            return;
        };
        let title = request.title.read(cx).text().trim().to_string();
        if title.is_empty() || request.sending || self.delivery.busy || !request.forge_ready() {
            return;
        }
        let body = request.body.read(cx).text().trim().to_string();
        let include_local = request.include_local && request.local.0 > 0;
        let draft = request.draft;
        request.sending = true;
        self.delivery.busy = true;
        let api = self.api();
        cx.notify();
        self._delivery = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    let changes_token = if include_local {
                        Some(api.changes_token(&id)?)
                    } else {
                        None
                    };
                    let input = RequestInput {
                        title: Some(title),
                        body: Some(body).filter(|body| !body.is_empty()),
                        draft,
                        changes_token,
                    };
                    api.create_request_with(&id, &input)
                })
                .await;
            let _ = this.update(cx, |column, cx| {
                column.delivery.busy = false;
                match result {
                    Ok(()) => {
                        column.delivery.request = None;
                        column.delivery.drafting = None;
                        let text = cx.t_with(
                            "composer.executionDock.createRequest.created",
                            &Args::default().with("requestLabel", label),
                        );
                        Toaster::success(text, cx);
                    }
                    Err(error) => {
                        if let Some(request) = column.delivery.request.as_mut() {
                            request.sending = false;
                        }
                        dock_failed(error, cx);
                    }
                }
                column.refresh_git(cx);
                cx.notify();
            });
        }));
    }

    /// `dialogRequestLabel`: the context's, else the forge's.
    fn request_label_for_dialog(&self) -> &'static str {
        let context = self
            .delivery
            .request
            .as_ref()
            .and_then(|request| request.context.as_ref());
        match context.map(|context| context.request_label.as_str()) {
            Some("MR") => "MR",
            _ => self.delivery.request_label(),
        }
    }

    /// `headBranch` → `baseBranch` for the description.
    fn request_route(&self) -> (String, String) {
        let context = self
            .delivery
            .request
            .as_ref()
            .and_then(|request| request.context.as_ref());
        let head = context
            .map(|context| context.head_branch.clone())
            .filter(|head| !head.is_empty())
            .or_else(|| self.current_branch())
            .unwrap_or_else(|| "HEAD".into());
        let branch_base = match &self.changes.branch {
            Loaded::Ready(diff) => diff.base_branch.clone(),
            _ => None,
        };
        let base = context
            .map(|context| context.base_branch.clone())
            .filter(|base| !base.is_empty())
            .or(branch_base)
            .or_else(|| {
                self.delivery
                    .task
                    .as_ref()
                    .map(|task| task.base_branch.clone())
            })
            .unwrap_or_else(|| "main".into());
        (head, base)
    }
}

fn check_row(
    id: &'static str,
    checked: bool,
    draft: bool,
    content: AnyElement,
    cx: &mut Context<CodeColumn>,
) -> gpui::Div {
    let entity = cx.entity().downgrade();
    let toggle = move |checked: bool, _: &mut Window, cx: &mut gpui::App| {
        let _ = entity.update(cx, |column, cx| column.toggle_request(draft, checked, cx));
    };
    div()
        .flex()
        .gap(px(8.0))
        .text_size(px(12.0))
        .child(Checkbox::new(id, checked).on_toggle(toggle))
        .child(content)
}

/// The dialog while it is open.
pub(super) fn request_dialog(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let request = column.delivery.request.as_ref()?;
    let colors = cx.theme().colors.clone();
    let label_text = column.request_label_for_dialog();
    let args = Args::default().with("requestLabel", label_text);
    let (head, base) = column.request_route();
    let forge = request.forge.as_ref().and_then(Option::as_ref);
    let destination = forge.and_then(|forge| {
        let destination = forge.destination()?;
        Some(match forge.effective_login.clone() {
            Some(login) => cx.t_with(
                "composer.executionDock.createRequest.destinationAs",
                &Args::default()
                    .with("destination", destination)
                    .with("login", login),
            ),
            None => cx.t_with(
                "composer.executionDock.createRequest.destination",
                &Args::default().with("destination", destination),
            ),
        })
    });
    let route = cx.t_with(
        "composer.executionDock.createRequest.route",
        &Args::default().with("head", head).with("base", base),
    );
    let header = div()
        .child(dialog_header(
            cx.t_with("composer.executionDock.createRequest.title", &args),
            Some(route),
            cx,
        ))
        .when_some(destination, |header, destination| {
            header.child(
                div()
                    .mt(px(4.0))
                    .font_family(fonts::MONO)
                    .text_size(px(11.0))
                    .text_color(colors.foreground.opacity(0.8))
                    .child(destination),
            )
        });
    let ready = request.forge_ready();
    // The forge answered and cannot open one here.
    let error = (request.forge.is_some() && !ready).then(|| {
        div()
            .text_size(px(12.0))
            .text_color(colors.destructive)
            .child(cx.t_with("turnReview.delivery.connectForge", &args))
    });
    let input_focus: FocusHandle = request.title.read(cx).focus_handle(cx);
    let title_field = field(
        request.title.clone().into_any_element(),
        &input_focus,
        None,
        false,
        window,
        cx,
    );
    let body_focus = request.body.read(cx).focus_handle(cx);
    let body_field = field(
        request.body.clone().into_any_element(),
        &body_focus,
        Some(96.0),
        false,
        window,
        cx,
    );
    let (files, additions, deletions) = request.local;
    let include = (files > 0).then(|| {
        let summary = cx.t_with(
            "composer.executionDock.createRequest.localSummary",
            &Args::default()
                .with("files", files.to_string())
                .with("additions", additions.to_string())
                .with("deletions", deletions.to_string()),
        );
        let content = div()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(cx.t("composer.executionDock.createRequest.includeLocalChanges")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_color(colors.muted_foreground)
                    .child(summary),
            )
            .into_any_element();
        check_row(
            "request-include-local",
            request.include_local,
            false,
            content,
            cx,
        )
        .items_start()
        .rounded(px(10.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.2))
        .p(px(12.0))
    });
    let draft_check_label = div()
        .font_weight(FontWeight::MEDIUM)
        .child(cx.t("composer.executionDock.createRequest.draft"))
        .into_any_element();
    let draft =
        check_row("request-draft", request.draft, true, draft_check_label, cx).items_center();
    let fields = div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .py(px(16.0))
        .children(error)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(draft_label(
                    cx.t("composer.executionDock.createRequest.titleLabel"),
                    DraftField::RequestTitle,
                    column,
                    cx,
                ))
                .child(title_field),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(draft_label(
                    cx.t("composer.executionDock.createRequest.descriptionLabel"),
                    DraftField::RequestBody,
                    column,
                    cx,
                ))
                .child(body_field),
        )
        .children(include)
        .child(draft);
    let sending = request.sending || column.delivery.busy;
    let empty = request.title.read(cx).text().trim().is_empty();
    let submit: SharedString = if sending {
        cx.t("composer.executionDock.createRequest.creating")
    } else {
        cx.t_with("composer.executionDock.createRequest.submit", &args)
    };
    let footer = dialog_footer(
        [
            Button::new("request-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(sending)
                .child(cx.t("composer.executionDock.createRequest.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.close_request(cx)))
                .into_any_element(),
            Button::new("request-submit")
                .size(ButtonSize::Default)
                .disabled(sending || !ready || empty)
                .child(submit)
                .on_click(cx.listener(|column, _, _, cx| column.send_request(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("create-request-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.close_request(cx));
        })
        .max_width(512.0)
        .gap(0.0)
        .child(header)
        .child(fields)
        .child(footer)
        .into_any_element(),
    )
}
