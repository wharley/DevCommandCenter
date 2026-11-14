//! What a routine's row and form offer beyond the schedule (`routines`):
//! "Rodar agora" sends the request at once, by the clock's own path, so the
//! holds apply and the run is marked (`shell::routines`); "Editar" opens the
//! form on the routine, saved over it; the form's examples fill the request
//! for a first routine; and the row says when it last ran.

use dcc_core::domain::agent_routine::Routine;
use gpui::{AnyElement, AppContext, Context, SharedString, Window, div, prelude::*, px};

use super::routines::RoutineForm;
use super::{AgentPage, AgentPageEvent};
use crate::data::time::medium_date_time;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, TextArea, Toaster};

/// The requests the form offers to start from.
const EXAMPLES: [&str; 3] = [
    "agents.routines.example.prs",
    "agents.routines.example.yesterday",
    "agents.routines.example.stuck",
];

impl AgentPage {
    /// "Última: …", once it ran.
    pub(super) fn routine_last_run(
        routine: &Routine,
        english: bool,
        cx: &gpui::App,
    ) -> Option<String> {
        let at = routine.last_run_at.as_deref()?;
        Some(
            cx.t_with(
                "agents.routines.last",
                &Args::default().with("when", medium_date_time(at, english)),
            )
            .to_string(),
        )
    }

    /// "Rodar agora" and "Editar" on a routine that is the person's.
    pub(super) fn routine_tools(
        &self,
        index: usize,
        routine: &Routine,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if routine.proposed {
            return Vec::new();
        }
        let (run, edit) = (routine.clone(), routine.clone());
        vec![
            Button::new(SharedString::from(format!("agent-routine-run-{index}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(AgentPageEvent::RunRoutine(Box::new(run.clone())));
                    Toaster::info(cx.t("agents.routines.running"), cx);
                }))
                .child(cx.t("agents.routines.runNow"))
                .into_any_element(),
            Button::new(SharedString::from(format!("agent-routine-edit-{index}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .on_click(
                    cx.listener(move |page, _, window, cx| page.edit_routine(&edit, window, cx)),
                )
                .child(cx.t("agents.routines.edit"))
                .into_any_element(),
        ]
    }

    /// The form on `routine`: saving it changes that one.
    pub(super) fn edit_routine(
        &mut self,
        routine: &Routine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let time = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("09:00")
                .with_max_len(5)
        });
        let at = format!("{:02}:{:02}", routine.hour, routine.minute);
        time.update(cx, |input, cx| input.set_text(at, cx));
        let prompt = cx.new(|cx| {
            TextArea::new(window, cx).with_placeholder(cx.t("agents.routines.promptPlaceholder"))
        });
        prompt.update(cx, |area, cx| area.set_text(&routine.prompt, cx));
        self.routine_form = Some(RoutineForm {
            days: routine.days.clone(),
            time,
            _changed: cx.observe(&prompt, |_, _, cx| cx.notify()),
            prompt,
            editing: Some((routine.id.clone(), routine.enabled)),
        });
        cx.notify();
    }

    /// The examples under a new routine's request, while it is empty.
    pub(super) fn routine_examples(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.routine_form.as_ref()?;
        if form.editing.is_some() || !form.prompt.read(cx).text().trim().is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        Some(
            div()
                .debug_selector(|| "agent-routine-examples".into())
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .children(EXAMPLES.iter().enumerate().map(|(index, key)| {
                    let text = cx.t(key);
                    let picked = text.clone();
                    div()
                        .id(SharedString::from(format!("routine-example-{index}")))
                        .debug_selector(move || format!("routine-example-{index}"))
                        .px(px(10.0))
                        .py(px(4.0))
                        .rounded_full()
                        .border_1()
                        .border_color(colors.border)
                        .text_size(px(12.0))
                        .text_color(colors.muted_foreground)
                        .cursor_pointer()
                        .hover(|chip| chip.text_color(colors.foreground))
                        .on_click(cx.listener(move |page, _, _, cx| {
                            if let Some(form) = &page.routine_form {
                                let prompt = form.prompt.clone();
                                prompt.update(cx, |area, cx| area.set_text(&picked, cx));
                                cx.notify();
                            }
                        }))
                        .child(text)
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::agent_routine::Routine;
    use gpui::{AppContext, Modifiers, TestAppContext, WindowAppearance};

    use super::super::{AgentPage, AgentPageEvent};
    use crate::data::loader::SidebarData;
    use crate::data::routines::Routines;
    use crate::screens::composer::Composer;
    use crate::screens::sidebar::Sidebar;
    use crate::theme::{Theme, ThemePreference};

    #[gpui::test]
    fn a_routine_runs_now_is_edited_and_a_new_one_starts_from_an_example(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
            crate::ui::text_input::bind_keys(cx);
            cx.set_global(Routines {
                list: vec![Routine {
                    id: "r-1".into(),
                    agent_id: "sup".into(),
                    days: vec![0, 2],
                    hour: 9,
                    minute: 30,
                    prompt: "o que travou?".into(),
                    enabled: false,
                    proposed: false,
                    last_run_at: Some("2026-11-13T12:30:00Z".into()),
                    created_at: "2026-11-01T10:00:00Z".into(),
                }],
            });
        });
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("seen.json");
        let (page, cx) = cx.add_window_view(move |window, cx| {
            let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
            let composer = cx.new(|cx| Composer::new(window, cx));
            AgentPage::new("sup".into(), sidebar, composer, cx)
        });
        cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(2400.0)));
        let mut suporte = super::super::editor_role::blank_agent();
        suporte.id = "sup".into();
        suporte.name = "Suporte".into();
        let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = ran.clone();
        page.update(cx, |page, cx| {
            page.sidebar.update(cx, |sidebar, cx| {
                sidebar.apply_for_tests(
                    SidebarData {
                        agents: vec![suporte],
                        ..SidebarData::default()
                    },
                    cx,
                )
            });
            cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
                if let AgentPageEvent::RunRoutine(routine) = event {
                    sink.borrow_mut().push(routine.id.clone());
                }
            })
            .detach();
            page.show("sup".into(), cx);
        });
        cx.run_until_parked();
        let click = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} on screen"));
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        };
        // "Rodar agora" asks the shell to send it, by the clock's path.
        click(cx, "agent-routine-run-0");
        assert_eq!(*ran.borrow(), ["r-1"]);

        // "Editar": the form on that routine, with no examples; saving
        // keeps its id and its switch.
        click(cx, "agent-routine-edit-0");
        assert!(cx.debug_bounds("agent-routine-form").is_some());
        assert!(cx.debug_bounds("agent-routine-examples").is_none());
        page.read_with(cx, |page, cx| {
            let form = page.routine_form.as_ref().unwrap();
            assert_eq!(form.days, [0, 2]);
            assert_eq!(form.time.read(cx).text(), "09:30");
            assert_eq!(form.prompt.read(cx).text(), "o que travou?");
            assert_eq!(form.editing, Some(("r-1".into(), false)));
        });
        page.update(cx, |page, cx| {
            let prompt = page.routine_form.as_ref().unwrap().prompt.clone();
            prompt.update(cx, |area, cx| {
                area.set_text("o que travou no Checkout?", cx)
            });
        });
        click(cx, "agent-routine-save");
        cx.update(|_, cx| {
            let list = &cx.global::<Routines>().list;
            assert_eq!(list.len(), 1, "changed in place, not added");
            assert_eq!(list[0].id, "r-1");
            assert_eq!(list[0].prompt, "o que travou no Checkout?");
            assert!(!list[0].enabled);
        });

        // A new one offers examples until something is written.
        click(cx, "agent-routine-new");
        assert!(cx.debug_bounds("agent-routine-examples").is_some());
        click(cx, "routine-example-0");
        page.read_with(cx, |page, cx| {
            let form = page.routine_form.as_ref().unwrap();
            assert_eq!(
                form.prompt.read(cx).text(),
                "Resuma os PRs abertos que esperam por mim."
            );
        });
        assert!(cx.debug_bounds("agent-routine-examples").is_none());
    }
}
