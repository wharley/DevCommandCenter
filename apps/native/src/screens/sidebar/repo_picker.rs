//! `WorkspaceRepoPicker`: the + button that starts a task in a project,
//! with a search field over the projects (pinned first) and two actions.
//! cmdk ranks matches by a fuzzy score; here a match keeps the list order.

use gpui::{
    AnyElement, AppContext, Context, KeyDownEvent, SharedString, Subscription, Window, div,
    prelude::*, px,
};

use super::{Sidebar, SidebarEvent};
use crate::data::tasks::Project;
use crate::i18n::Translate;
use crate::project::ProjectGlyph;
use crate::theme::ActiveTheme;
use crate::theme::Appearance;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverAlign};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, TextInput, TextInputEvent};

/// Where the picker's button sits: the tasks header or the collapsed rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Header,
    Rail,
}

pub struct RepoPicker {
    place: Place,
    input: gpui::Entity<TextInput>,
    highlighted: usize,
    _events: Subscription,
}

#[derive(Clone, Debug, PartialEq)]
enum Choice {
    Project(Box<Project>),
    MultiProject,
    Clone,
}

/// cmdk's loose match: every query letter appears in `value`, in order.
pub fn matches(query: &str, value: &str) -> bool {
    let value = value.to_lowercase();
    let mut letters = value.chars();
    query
        .to_lowercase()
        .chars()
        .filter(|letter| !letter.is_whitespace())
        .all(|letter| letters.any(|candidate| candidate == letter))
}

/// The projects, pinned first (`orderedRepositories`).
fn ordered(projects: &[Project]) -> Vec<Project> {
    let mut projects = projects.to_vec();
    projects.sort_by_key(|project| project.pinned_at.is_none());
    projects
}

impl Sidebar {
    fn choices(&self, cx: &gpui::App) -> (Vec<Choice>, Vec<Choice>) {
        let query = self
            .repo_picker
            .as_ref()
            .map(|picker| picker.input.read(cx).text().to_string())
            .unwrap_or_default();
        let projects = ordered(&self.state.data.projects)
            .into_iter()
            .filter(|project| {
                let value = format!(
                    "{} {} {} {} {}",
                    project.display_name,
                    project.name,
                    project.project_id,
                    project.base_branch,
                    project.root_path
                );
                matches(&query, &value)
            })
            .map(|project| Choice::Project(Box::new(project)))
            .collect();
        let actions = [
            (Choice::MultiProject, "create workspace"),
            (Choice::Clone, "clone from url"),
        ]
        .into_iter()
        .filter(|(_, value)| matches(&query, value))
        .map(|(choice, _)| choice)
        .collect();
        (projects, actions)
    }

    fn set_repo_picker_open(
        &mut self,
        place: Place,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !open {
            self.repo_picker = None;
            cx.notify();
            return;
        }
        let input = cx.new(|cx| {
            TextInput::new(window, cx).with_placeholder(cx.t("sidebar.searchReposPlaceholder"))
        });
        input.update(cx, |input, cx| input.focus_and_select_all(window, cx));
        let events = cx.subscribe_in(
            &input,
            window,
            move |this, _, event: &TextInputEvent, window, cx| match event {
                TextInputEvent::Changed => {
                    if let Some(picker) = this.repo_picker.as_mut() {
                        picker.highlighted = 0;
                    }
                    cx.notify();
                }
                TextInputEvent::Submitted => {
                    let index = this
                        .repo_picker
                        .as_ref()
                        .map_or(0, |picker| picker.highlighted);
                    this.choose(index, window, cx);
                }
                TextInputEvent::Cancelled => this.set_repo_picker_open(place, false, window, cx),
                TextInputEvent::Blurred => {}
            },
        );
        self.repo_picker = Some(RepoPicker {
            place,
            input,
            highlighted: 0,
            _events: events,
        });
        cx.notify();
    }

    fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        let (projects, actions) = self.choices(cx);
        let count = projects.len() + actions.len();
        let Some(picker) = self.repo_picker.as_mut() else {
            return;
        };
        if count == 0 {
            return;
        }
        // cmdk does not wrap around by default.
        picker.highlighted = picker
            .highlighted
            .saturating_add_signed(step)
            .min(count - 1);
        cx.notify();
    }

    fn choose(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let (projects, actions) = self.choices(cx);
        let Some(choice) = projects.into_iter().chain(actions).nth(index) else {
            return;
        };
        self.repo_picker = None;
        cx.notify();
        cx.emit(match choice {
            Choice::Project(project) => SidebarEvent::NewTaskInProject {
                project_id: project.project_id,
                root_path: project.root_path,
                base_branch: project.base_branch,
                label: project.display_name,
            },
            Choice::MultiProject => SidebarEvent::OpenProject,
            Choice::Clone => SidebarEvent::CloneProject,
        });
    }
}

/// `CommandGroup` heading (`px-2 py-1.5 text-xs font-medium text-muted-foreground`).
fn group_heading(label: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(cx.theme().colors.muted_foreground)
        .child(label)
        .into_any_element()
}

/// `CommandItem`: highlighted by the pointer or the arrows, picked by click or Enter.
fn item(
    index: usize,
    highlighted: bool,
    content: AnyElement,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(("repo-choice", index))
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(theme.radius.sm)
        .cursor_pointer()
        .when(highlighted, |item| {
            item.bg(theme.colors.muted)
                .text_color(theme.colors.foreground)
        })
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered && let Some(picker) = this.repo_picker.as_mut() {
                picker.highlighted = index;
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |this, _, window, cx| this.choose(index, window, cx)))
        .child(content)
        .into_any_element()
}

fn project_content(project: &Project, cx: &gpui::App) -> AnyElement {
    let muted = cx.theme().colors.muted_foreground;
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .child(ProjectGlyph::new(
            project.icon.as_deref(),
            project.color.as_deref(),
            Some(&project.root_path),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(SharedString::from(project.display_name.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_color(muted)
                        .child(SharedString::from(project.base_branch.clone())),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(10.0))
                        .line_height(px(15.0))
                        .text_color(muted)
                        .child(SharedString::from(project.root_path.clone())),
                ),
        )
        .into_any_element()
}

/// "Nova tarefa multi-projeto" with its emerald tile and description.
fn multi_project_content(cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    let tile_text = match cx.theme().appearance {
        Appearance::Light => tw::EMERALD_600.hsla(),
        Appearance::Dark => tw::EMERALD_400.hsla(),
    };
    div()
        .flex()
        .items_start()
        .gap(px(8.0))
        .py(px(2.0))
        .child(
            div()
                .mt(px(2.0))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(24.0))
                .rounded(cx.theme().radius.md)
                .bg(tw::EMERALD_500.alpha(0.1))
                .text_color(tile_text)
                .child(Icon::new("plus").size(14.0)),
        )
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(cx.t("sidebar.openRepoPickerAction")),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(10.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("sidebar.multiProjectActionDescription")),
                ),
        )
        .into_any_element()
}

fn list(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> AnyElement {
    let (projects, actions) = sidebar.choices(cx);
    let highlighted = sidebar
        .repo_picker
        .as_ref()
        .map_or(0, |picker| picker.highlighted);
    let colors = cx.theme().colors.clone();
    let mut body: Vec<AnyElement> = Vec::new();
    if projects.is_empty() && actions.is_empty() {
        body.push(
            div()
                .py(px(24.0))
                .text_center()
                .child(cx.t("sidebar.noRepoFound"))
                .into_any_element(),
        );
    }
    let mut index = 0;
    if !projects.is_empty() {
        let mut group = vec![group_heading(cx.t("sidebar.recentRepos"), cx)];
        for choice in &projects {
            if let Choice::Project(project) = choice {
                let content = project_content(project, cx);
                group.push(item(index, index == highlighted, content, cx));
            }
            index += 1;
        }
        body.push(div().p(px(4.0)).children(group).into_any_element());
    }
    if !actions.is_empty() {
        if !projects.is_empty() {
            body.push(
                div()
                    .mx(px(-4.0))
                    .h(px(1.0))
                    .bg(colors.border)
                    .into_any_element(),
            );
        }
        let mut group = vec![group_heading(cx.t("sidebar.actions"), cx)];
        for choice in &actions {
            let content = match choice {
                Choice::MultiProject => multi_project_content(cx),
                _ => div().child(cx.t("sidebar.cloneFromUrl")).into_any_element(),
            };
            group.push(item(index, index == highlighted, content, cx));
            index += 1;
        }
        body.push(div().p(px(4.0)).children(group).into_any_element());
    }
    div()
        .id("repo-picker-list")
        .max_h(px(288.0))
        .overflow_y_scroll()
        .children(body)
        .into_any_element()
}

/// The `CommandInput`: a search icon and the field in a soft box.
fn search_field(input: gpui::Entity<TextInput>, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .p(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(32.0))
                .px(px(10.0))
                .rounded(cx.theme().radius.lg)
                .border_1()
                .border_color(colors.input.opacity(0.3))
                .bg(colors.input.opacity(0.3))
                .text_color(colors.foreground)
                .child(div().opacity(0.5).child(Icon::new("search")))
                .child(input),
        )
        .into_any_element()
}

pub fn render(sidebar: &Sidebar, place: Place, cx: &mut Context<Sidebar>) -> AnyElement {
    let open = sidebar
        .repo_picker
        .as_ref()
        .is_some_and(|picker| picker.place == place);
    let entity = cx.entity().downgrade();
    let content_entity = entity.clone();
    let trigger = Button::new(SharedString::from(format!("open-repo-picker-{place:?}")))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .child(Icon::new("plus").stroke(2.2));
    Popover::new(
        SharedString::from(format!("repo-picker-{place:?}")),
        open,
        trigger,
        move |_, cx| {
            content_entity
                .update(cx, |sidebar, cx| {
                    let Some(picker) = sidebar.repo_picker.as_ref() else {
                        return div().into_any_element();
                    };
                    let field = search_field(picker.input.clone(), cx);
                    // Command: `rounded-xl p-1`, transparent on the popover.
                    div()
                        .p(px(4.0))
                        .rounded(cx.theme().radius.xl)
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            match event.keystroke.key.as_str() {
                                "down" => this.move_highlight(1, cx),
                                "up" => this.move_highlight(-1, cx),
                                _ => return,
                            }
                            cx.stop_propagation();
                        }))
                        .child(field)
                        .child(list(sidebar, cx))
                        .into_any_element()
                })
                .unwrap_or_else(|_| div().into_any_element())
        },
        move |open, window, cx| {
            let _ = entity.update(cx, |sidebar, cx| {
                sidebar.set_repo_picker_open(place, open, window, cx)
            });
        },
    )
    .align(PopoverAlign::End)
    .side_offset(8.0)
    .width(320.0)
    .padding(0.0)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn letters_match_in_order() {
        assert!(matches("", "anything"));
        assert!(matches("dcc", "DevCommandCenter main"));
        assert!(matches("dev cc", "DevCommandCenter"));
        assert!(!matches("ccd", "dcc"));
        assert!(matches("clone", "clone from url"));
    }
}
