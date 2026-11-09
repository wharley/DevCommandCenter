//! "Rotinas" on an agent's page (`dcc_core::domain::agent_routine`): each
//! routine with its days and time, when it runs next, a switch and
//! "Apagar"; a proposal from the agent with "Criar"; and "Nova rotina" —
//! the days, a time (`09:00`) and the request — saved through the desktop.
//! The list is the `Routines` global the clock reads once a minute.

use chrono::Local;
use dcc_core::domain::agent_routine::{MAX_ROUTINES, Routine, RoutineDraft};
use gpui::{
    AnyElement, AppContext, BorrowAppContext, Context, Entity, FontWeight, SharedString, Window,
    div, prelude::*, px,
};

use super::AgentPage;
use crate::data::routines::{Routines, schedule_label};
use crate::data::time::medium_date_time;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, Switch, TextArea, Toaster};

/// "Nova rotina", while open.
pub(crate) struct RoutineForm {
    pub(crate) days: Vec<u8>,
    pub(crate) time: Entity<TextInput>,
    pub(crate) prompt: Entity<TextArea>,
}

/// `09:00` → (9, 0).
pub(crate) fn parse_time(text: &str) -> Option<(u8, u8)> {
    let (hour, minute) = text.trim().split_once(':')?;
    let (hour, minute) = (hour.parse::<u8>().ok()?, minute.parse::<u8>().ok()?);
    (hour < 24 && minute < 60).then_some((hour, minute))
}

impl AgentPage {
    fn routines(&self, cx: &gpui::App) -> Vec<Routine> {
        cx.try_global::<Routines>()
            .map(|routines| routines.of(&self.agent_id).cloned().collect())
            .unwrap_or_default()
    }

    fn open_routine_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let time = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("09:00")
                .with_max_len(5)
        });
        time.update(cx, |input, cx| input.set_text("09:00", cx));
        let prompt = cx.new(|cx| {
            TextArea::new(window, cx).with_placeholder(cx.t("agents.routines.promptPlaceholder"))
        });
        self.routine_form = Some(RoutineForm {
            days: vec![0, 1, 2, 3, 4],
            time,
            prompt,
        });
        cx.notify();
    }

    /// Changes the list at once; the desktop keeps it and the clock reads it.
    fn update_routines(&self, cx: &mut Context<Self>, change: impl FnOnce(&mut Vec<Routine>)) {
        if cx.has_global::<Routines>() {
            cx.update_global::<Routines, _>(|routines, _| change(&mut routines.list));
        }
    }

    fn call(
        &self,
        cx: &mut Context<Self>,
        job: impl FnOnce(
            crate::data::desktop_api::DesktopApi,
        ) -> Result<(), crate::data::desktop_api::ApiError>
        + Send
        + 'static,
    ) {
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        cx.spawn(async move |_, cx| {
            // The list read again right after, with the ids the desktop gave.
            let done = cx
                .background_executor()
                .spawn(async move { job(api).map(|()| crate::data::routines::read()) })
                .await;
            let _ = cx.update(|cx| match done {
                Ok(list) => cx.set_global(Routines { list }),
                Err(error) => Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    fn save_routine_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.routine_form else {
            return;
        };
        let Some((hour, minute)) = parse_time(form.time.read(cx).text()) else {
            Toaster::error(cx.t("agents.routines.badTime"), cx);
            return;
        };
        let draft = match (RoutineDraft {
            days: form.days.clone(),
            hour,
            minute,
            prompt: form.prompt.read(cx).text().to_string(),
        })
        .normalized()
        {
            Ok(draft) => draft,
            Err(error) => {
                Toaster::error(error, cx);
                return;
            }
        };
        let agent = self.agent_id.clone();
        self.update_routines(cx, |list| {
            list.push(Routine {
                id: format!("pending-{}", list.len()),
                agent_id: agent.clone(),
                days: draft.days.clone(),
                hour,
                minute,
                prompt: draft.prompt.clone(),
                enabled: true,
                proposed: false,
                last_run_at: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            })
        });
        self.call(cx, move |api| api.save_routine(&agent, None, &draft, true));
        self.routine_form = None;
        cx.notify();
    }

    fn day_chips(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let names: [&str; 7] = if english {
            ["M", "T", "W", "T", "F", "S", "S"]
        } else {
            ["S", "T", "Q", "Q", "S", "S", "D"]
        };
        let picked = self
            .routine_form
            .as_ref()
            .map(|form| form.days.clone())
            .unwrap_or_default();
        div()
            .flex()
            .gap(px(4.0))
            .children(names.iter().enumerate().map(|(day, name)| {
                let on = picked.contains(&(day as u8));
                div()
                    .id(SharedString::from(format!("routine-day-{day}")))
                    .debug_selector(move || format!("routine-day-{day}"))
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border)
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .when(on, |chip| {
                        chip.bg(colors.accent).text_color(colors.foreground)
                    })
                    .when(!on, |chip| chip.text_color(colors.muted_foreground))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        if let Some(form) = &mut page.routine_form {
                            let day = day as u8;
                            if form.days.contains(&day) {
                                form.days.retain(|picked| *picked != day);
                            } else {
                                form.days.push(day);
                            }
                            cx.notify();
                        }
                    }))
                    .child(*name)
            }))
            .into_any_element()
    }

    fn routine_row(&self, index: usize, routine: &Routine, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let english = cx.global::<I18n>().locale() == Locale::En;
        let next = routine
            .next_run(Local::now())
            .map(|at| medium_date_time(&at.to_rfc3339(), english));
        let detail = if routine.proposed {
            cx.t("agents.routines.proposedBadge").to_string()
        } else {
            match next {
                Some(next) => cx
                    .t_with("agents.routines.next", &Args::default().with("when", next))
                    .to_string(),
                None => cx.t("agents.routines.off").to_string(),
            }
        };
        let (agent, id, on) = (
            routine.agent_id.clone(),
            routine.id.clone(),
            routine.enabled,
        );
        let toggled = routine.clone();
        let (accept_agent, accept_id) = (agent.clone(), id.clone());
        let page = cx.entity().downgrade();
        div()
            .debug_selector(move || format!("agent-routine-{index}"))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(schedule_label(routine, english)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(routine.prompt.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(detail),
                    ),
            )
            .when(routine.proposed, |row| {
                row.child(
                    Button::new(SharedString::from(format!("agent-routine-create-{index}")))
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(move |page, _, _, cx| {
                            let (agent, id) = (accept_agent.clone(), accept_id.clone());
                            let marked = id.clone();
                            page.update_routines(cx, |list| {
                                for found in list.iter_mut().filter(|found| found.id == marked) {
                                    found.proposed = false;
                                    found.enabled = true;
                                }
                            });
                            page.call(cx, move |api| api.accept_routine(&agent, &id));
                            cx.notify();
                        }))
                        .child(cx.t("agents.routines.create")),
                )
            })
            .when(!routine.proposed, |row| {
                row.child(
                    Switch::new(
                        SharedString::from(format!("agent-routine-switch-{index}")),
                        on,
                    )
                    .on_toggle(move |on, _, cx| {
                        let routine = toggled.clone();
                        let _ = page.update(cx, |page, cx| {
                            let id = routine.id.clone();
                            page.update_routines(cx, |list| {
                                for found in list.iter_mut().filter(|found| found.id == id) {
                                    found.enabled = on;
                                }
                            });
                            let draft = RoutineDraft {
                                days: routine.days.clone(),
                                hour: routine.hour,
                                minute: routine.minute,
                                prompt: routine.prompt.clone(),
                            };
                            let agent = routine.agent_id.clone();
                            page.call(cx, move |api| {
                                api.save_routine(&agent, Some(&routine.id), &draft, on)
                            });
                            cx.notify();
                        });
                    }),
                )
            })
            .child(
                Button::new(SharedString::from(format!("agent-routine-delete-{index}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(move |page, _, _, cx| {
                        let (agent, id) = (agent.clone(), id.clone());
                        let gone = id.clone();
                        page.update_routines(cx, |list| list.retain(|found| found.id != gone));
                        page.call(cx, move |api| api.delete_routine(&agent, &id));
                        cx.notify();
                    }))
                    .child(cx.t("agents.routines.delete")),
            )
            .into_any_element()
    }

    pub(super) fn routines_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let routines = self.routines(cx);
        let rows: Vec<AnyElement> = routines
            .iter()
            .enumerate()
            .map(|(index, routine)| self.routine_row(index, routine, cx))
            .collect();
        let full = routines.len() >= MAX_ROUTINES;
        let form = self.routine_form.as_ref().map(|form| {
            let (time, prompt) = (form.time.clone(), form.prompt.clone());
            div()
                .debug_selector(|| "agent-routine-form".into())
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(12.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(12.0))
                        .child(self.day_chips(cx))
                        .child(
                            div()
                                .debug_selector(|| "agent-routine-time".into())
                                .w(px(72.0))
                                .h(px(30.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(colors.input)
                                .bg(crate::ui::input_fill(cx.theme()))
                                .text_size(px(13.0))
                                .font_features(crate::ui::tabular_nums())
                                .child(time),
                        ),
                )
                .child(
                    div()
                        .debug_selector(|| "agent-routine-prompt".into())
                        .min_h(px(64.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.input)
                        .bg(crate::ui::input_fill(cx.theme()))
                        .text_size(px(13.0))
                        .child(prompt),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            Button::new("agent-routine-cancel")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(|page, _, _, cx| {
                                    page.routine_form = None;
                                    cx.notify();
                                }))
                                .child(cx.t("agents.routines.cancel")),
                        )
                        .child(
                            Button::new("agent-routine-save")
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(|page, _, _, cx| page.save_routine_form(cx)))
                                .child(cx.t("agents.routines.save")),
                        ),
                )
        });
        let add = (self.routine_form.is_none() && !full).then(|| {
            Button::new("agent-routine-new")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .on_click(cx.listener(|page, _, window, cx| page.open_routine_form(window, cx)))
                .child(cx.t("agents.routines.new"))
        });
        Self::card(cx)
            .debug_selector(|| "agent-routines".into())
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(20.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(cx.t("agents.routines.title")),
                    )
                    .children(add),
            )
            .when(rows.is_empty() && form.is_none(), |card| {
                card.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(19.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("agents.routines.empty")),
                )
            })
            .children(rows)
            .children(form)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_as_hours_and_minutes() {
        assert_eq!(parse_time("09:30"), Some((9, 30)));
        assert_eq!(parse_time(" 7:05 "), Some((7, 5)));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("9h"), None);
    }
}
