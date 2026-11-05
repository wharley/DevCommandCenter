//! `WorkspaceDeliveryControls`, after the Browser: "Revisar mudanças da
//! tarefa" with the local +/− (the branch's when the tree is clean), and
//! the delivery menu named after its next step ("Ações git" with every
//! step, the next one in bold). A view of its own: the shell sets it from
//! the code column and hears its picks, without the conversation drawn
//! again each time git is read.

use dcc_core::domain::commit_mode::{CommitButtonStatus, commit_translation_key};
use dcc_core::domain::execution_dock::{DockAction, DockActionId, DockRunMode, DockStatus};
use gpui::{
    AnyElement, App, Context, EventEmitter, Render, SharedString, Window, div, prelude::*, px,
};

use crate::i18n::{Args, Translate};
use crate::screens::code_column::delivery::DockControls;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Icon, Menu, MenuAlign, MenuEntry, MenuItem, spinner,
    tabular_nums,
};

/// `actionLabel`
pub(crate) fn action_label(action: &DockAction, request_label: &str, cx: &App) -> SharedString {
    let args = Args::default().with("requestLabel", request_label);
    match (action.id, action.mode) {
        (DockActionId::SyncBase, _) | (_, Some(DockRunMode::SyncBase)) => {
            cx.t("composer.executionDock.actions.syncBase")
        }
        (DockActionId::Pull, _) | (_, Some(DockRunMode::Pull)) => {
            cx.t("composer.executionDock.actions.pull")
        }
        (DockActionId::CreateDraftPr, _) => {
            cx.t_with("composer.executionDock.actions.createDraft", &args)
        }
        (_, Some(DockRunMode::Commit)) => cx.t("composer.executionDock.actions.commit"),
        (_, Some(DockRunMode::Mode(mode))) => cx.t_with(
            &commit_translation_key(mode, CommitButtonStatus::Idle),
            &args,
        ),
        (_, None) => cx.t("composer.executionDock.actions.git"),
    }
}

pub enum DeliveryButtonsEvent {
    /// "Revisar mudanças da tarefa": the code column on "Mudanças".
    ReviewChanges,
    /// A step of the menu, for the code column to run.
    Run(DockAction),
}

/// The two buttons; nothing without a task's checkout.
#[derive(Default)]
pub struct DeliveryButtons {
    pub controls: Option<DockControls>,
}

impl EventEmitter<DeliveryButtonsEvent> for DeliveryButtons {}

impl DeliveryButtons {
    pub fn set(&mut self, controls: Option<DockControls>, cx: &mut Context<Self>) {
        if self.controls != controls {
            self.controls = controls;
            cx.notify();
        }
    }
}

impl Render for DeliveryButtons {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.buttons(cx) {
            Some(buttons) => buttons,
            None => div().into_any_element(),
        }
    }
}

impl DeliveryButtons {
    fn buttons(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let controls = self.controls.clone()?;
        let colors = cx.theme().colors.clone();
        let (additions, deletions) = controls.status.counts();
        let count: AnyElement = if controls.status == DockStatus::Loading {
            spinner("header-review-loading", 12.0).into_any_element()
        } else if additions > 0 || deletions > 0 {
            div()
                .flex()
                .font_features(tabular_nums())
                .when(additions > 0, |count| {
                    count.child(
                        div()
                            .text_color(tw::EMERALD_500.hsla())
                            .child(format!("+{additions}")),
                    )
                })
                .when(deletions > 0, |count| {
                    count.child(
                        div()
                            .ml(px(4.0))
                            .text_color(colors.destructive)
                            .child(format!("−{deletions}")),
                    )
                })
                .into_any_element()
        } else {
            div()
                .text_color(colors.muted_foreground.opacity(0.65))
                .child("0")
                .into_any_element()
        };
        let review = Button::new("header-review-changes")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .height(32.0)
            .padding_x(8.0)
            .text_size(11.0)
            .muted()
            .on_click(cx.listener(|_, _, _, cx| cx.emit(DeliveryButtonsEvent::ReviewChanges)))
            .child(Icon::new("file-diff").size(14.0).stroke(1.8))
            .child(count);
        let label = controls.request_label;
        let primary = controls
            .actions
            .first()
            .map(|action| action_label(action, label, cx))
            .unwrap_or_else(|| cx.t("composer.executionDock.actions.git"));
        let (busy, loading) = (controls.busy, controls.loading);
        let trigger = move |_open: bool| {
            div()
                .max_w(px(176.0))
                .child(
                    Button::new("header-delivery")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .height(32.0)
                        .padding_x(10.0)
                        .text_size(11.0)
                        .disabled(loading)
                        .when(busy, |button| {
                            button.child(spinner("header-delivery-busy", 12.0))
                        })
                        .child(div().min_w_0().truncate().child(primary.clone()))
                        .child(Icon::new("chevron-down").size(12.0)),
                )
                .into_any_element()
        };
        let title = cx.t("composer.executionDock.actions.title");
        let labels: Vec<SharedString> = controls
            .actions
            .iter()
            .map(|action| action_label(action, label, cx))
            .collect();
        let view = cx.entity().downgrade();
        let actions = controls.actions;
        let menu = Menu::dropdown("header-delivery-menu", trigger, move |_, _| {
            let mut entries = vec![MenuEntry::Label(title.clone())];
            for (action, text) in actions.iter().zip(&labels) {
                let view = view.clone();
                let picked = *action;
                entries.push(
                    MenuItem::new(text.clone(), move |_, cx| {
                        let _ = view.update(cx, |_, cx| cx.emit(DeliveryButtonsEvent::Run(picked)));
                    })
                    .disabled(action.disabled)
                    .emphasized(action.primary)
                    .into(),
                );
            }
            entries
        })
        .align(MenuAlign::End)
        .min_width(256.0);
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0))
                .child(review)
                .child(menu)
                .into_any_element(),
        )
    }
}
