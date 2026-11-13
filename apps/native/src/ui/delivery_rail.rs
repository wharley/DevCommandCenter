//! The delivery rail: a dot per step joined by a line, its name under it.
//! Done steps are green with a check, the current one an amber ring, a
//! stopped one red, the ones ahead quiet. The changes column and a pull
//! request's detail both draw it.

use gpui::{AnyElement, App, ClickEvent, FontWeight, SharedString, Window, div, prelude::*, px};

use crate::theme::{ActiveTheme, StateTone};
use crate::ui::Icon;

const DOT: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailTone {
    Done,
    Current,
    Failed,
    Todo,
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct RailStep {
    /// Stable, for the element's id and the tests' selector.
    pub key: &'static str,
    pub label: SharedString,
    pub tone: RailTone,
    pub on_click: Option<OnClick>,
}

impl RailStep {
    pub fn new(key: &'static str, label: impl Into<SharedString>, tone: RailTone) -> Self {
        Self {
            key,
            label: label.into(),
            tone,
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

fn dot(tone: RailTone, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let base = div()
        .flex()
        .flex_shrink_0()
        .size(px(DOT))
        .items_center()
        .justify_center()
        .rounded_full();
    match tone {
        RailTone::Done => base
            .bg(StateTone::Done.fill(theme))
            .child(
                Icon::new("check")
                    .size(9.0)
                    .stroke(3.0)
                    .color(gpui::white()),
            )
            .into_any_element(),
        RailTone::Failed => base
            .bg(StateTone::Failed.fill(theme))
            .child(Icon::new("x").size(9.0).stroke(3.0).color(gpui::white()))
            .into_any_element(),
        RailTone::Current => base
            .border_2()
            .border_color(StateTone::NeedsYou.text(theme))
            .into_any_element(),
        RailTone::Todo => base
            .border_1()
            .border_color(theme.colors.muted_foreground.opacity(0.45))
            .into_any_element(),
    }
}

/// The rail of `steps`, each taking the same share of the width.
pub fn delivery_rail(id: &'static str, steps: Vec<RailStep>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let done_line = StateTone::Done.fill(theme);
    let quiet_line = colors.muted_foreground.opacity(0.3);
    let last = steps.len().saturating_sub(1);
    let tones: Vec<RailTone> = steps.iter().map(|step| step.tone).collect();
    let columns = steps.into_iter().enumerate().map(|(index, step)| {
        // A line is green once the step it leaves is done.
        let line = |from: Option<&RailTone>, hidden: bool| {
            let color = if from == Some(&RailTone::Done) {
                done_line
            } else {
                quiet_line
            };
            div()
                .flex_1()
                .h(px(1.5))
                .when(!hidden, |line| line.bg(color))
        };
        let before = index.checked_sub(1).and_then(|index| tones.get(index));
        let label_color = match step.tone {
            RailTone::Done => colors.muted_foreground,
            RailTone::Current => StateTone::NeedsYou.text(theme),
            RailTone::Failed => StateTone::Failed.text(theme),
            RailTone::Todo => colors.muted_foreground.opacity(0.7),
        };
        let strong = matches!(step.tone, RailTone::Current | RailTone::Failed);
        let key = step.key;
        let hover = colors.foreground;
        div()
            .id(SharedString::from(format!("{id}-{key}")))
            .debug_selector(move || format!("{id}-{key}"))
            .flex()
            .flex_1()
            .min_w_0()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .w_full()
                    .items_center()
                    .child(line(before, index == 0))
                    .child(dot(step.tone, cx))
                    .child(line(Some(&step.tone), index == last)),
            )
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_size(px(10.0))
                    .line_height(px(13.0))
                    .text_color(label_color)
                    .when(strong, |label| label.font_weight(FontWeight::MEDIUM))
                    .child(step.label),
            )
            .when_some(step.on_click, |column, on_click| {
                column
                    .cursor_pointer()
                    .hover(move |style| style.text_color(hover))
                    .on_click(on_click)
            })
    });
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .flex()
        .w_full()
        .children(columns)
        .into_any_element()
}
