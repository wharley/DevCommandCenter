//! shadcn's DropdownMenu and ContextMenu (`components/ui/dropdown-menu.tsx`,
//! `context-menu.tsx`): a popover list of items, separators and submenus.
//! A dropdown opens on click under its trigger; a context menu opens on right
//! click at the pointer. Arrows, Enter and Esc work like Radix.

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, BoxShadow, ClickEvent, ElementId, FocusHandle, Hsla, KeyDownEvent,
    MouseButton, MouseDownEvent, Pixels, Point, SharedString, Window, anchored, deferred, div,
    point, prelude::*, px,
};

use super::Icon;

mod row;
use crate::theme::ActiveTheme;
use row::{Label, row};

pub type OnSelect = Rc<dyn Fn(&mut Window, &mut App)>;

pub enum MenuEntry {
    Item(MenuItem),
    Separator,
    /// A quiet heading over the items (the project name on a row's menu).
    Label(SharedString),
    Submenu {
        label: SharedString,
        icon: Option<Icon>,
        entries: Vec<MenuEntry>,
    },
}

pub struct MenuItem {
    label: SharedString,
    icon: Option<Icon>,
    disabled: bool,
    /// `text-destructive focus:text-destructive`
    destructive: bool,
    /// A quiet second line (`text-[11px] text-muted-foreground`).
    detail: Option<SharedString>,
    /// A mark at the right end (the current choice's ✓).
    trailing: Option<SharedString>,
    on_select: OnSelect,
}

impl MenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        on_select: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            icon: None,
            disabled: false,
            destructive: false,
            detail: None,
            trailing: None,
            on_select: Rc::new(on_select),
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn trailing(mut self, mark: impl Into<SharedString>) -> Self {
        self.trailing = Some(mark.into());
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

impl From<MenuItem> for MenuEntry {
    fn from(item: MenuItem) -> Self {
        MenuEntry::Item(item)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAlign {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opener {
    Click,
    RightClick,
}

/// Item text: the shadcn default is 14px with a 6px gap; the rail's menus
/// use `gap-2 text-[13px]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemStyle {
    pub text_size: f32,
    pub gap: f32,
}

impl Default for ItemStyle {
    fn default() -> Self {
        Self {
            text_size: 14.0,
            gap: 6.0,
        }
    }
}

#[derive(Default)]
struct MenuState {
    open: bool,
    /// Where a context menu opened.
    at: Option<Point<Pixels>>,
    highlighted: Option<usize>,
    open_submenu: Option<usize>,
    focus: Option<FocusHandle>,
    on_open_change: Option<OnOpenChange>,
}

type Entries = Rc<dyn Fn(&mut Window, &mut App) -> Vec<MenuEntry>>;
type Trigger = Box<dyn FnOnce(bool) -> AnyElement>;
type OnOpenChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Menu {
    id: ElementId,
    trigger: Trigger,
    entries: Entries,
    opener: Opener,
    align: MenuAlign,
    side_offset: f32,
    min_width: f32,
    item_style: ItemStyle,
    on_open_change: Option<OnOpenChange>,
}

impl Menu {
    /// A dropdown; `trigger` gets whether the menu is open (`aria-expanded`).
    pub fn dropdown(
        id: impl Into<ElementId>,
        trigger: impl FnOnce(bool) -> AnyElement + 'static,
        entries: impl Fn(&mut Window, &mut App) -> Vec<MenuEntry> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            trigger: Box::new(trigger),
            entries: Rc::new(entries),
            opener: Opener::Click,
            align: MenuAlign::Start,
            side_offset: 4.0,
            min_width: 96.0,
            item_style: ItemStyle::default(),
            on_open_change: None,
        }
    }

    /// A context menu around `target`, opened by a right click.
    pub fn context(
        id: impl Into<ElementId>,
        target: impl IntoElement + 'static,
        entries: impl Fn(&mut Window, &mut App) -> Vec<MenuEntry> + 'static,
    ) -> Self {
        let target = target.into_any_element();
        Self {
            opener: Opener::RightClick,
            min_width: 160.0,
            ..Self::dropdown(id, move |_| target, entries)
        }
    }

    pub fn align(mut self, align: MenuAlign) -> Self {
        self.align = align;
        self
    }

    pub fn side_offset(mut self, offset: f32) -> Self {
        self.side_offset = offset;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn item_style(mut self, style: ItemStyle) -> Self {
        self.item_style = style;
        self
    }

    /// Told when the menu opens or closes (a row keeps its actions shown).
    pub fn on_open_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

fn shadow_md(ring: Hsla) -> Vec<BoxShadow> {
    let shadow = |y: f32, blur: f32, spread: f32| BoxShadow {
        color: Hsla::black().opacity(0.1),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    vec![
        shadow(4.0, 6.0, -1.0),
        shadow(2.0, 4.0, -2.0),
        BoxShadow {
            color: ring,
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        },
    ]
}

impl RenderOnce for Menu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| MenuState::default());
        let on_open_change = self.on_open_change.clone();
        state.update(cx, |state, _| state.on_open_change = on_open_change);
        let open = state.read(cx).open;
        let opener = self.opener;
        let entries_builder = self.entries.clone();
        let trigger = (self.trigger)(open);

        let popup = open.then(|| {
            let entries = (entries_builder)(window, cx);
            let panel = panel(
                &state,
                &state,
                entries,
                self.min_width,
                self.item_style,
                window,
                cx,
            );
            let anchored = match (opener, state.read(cx).at) {
                (Opener::RightClick, Some(at)) => anchored()
                    .position(at)
                    .position_mode(gpui::AnchoredPositionMode::Window)
                    .anchor(Anchor::TopLeft)
                    .offset(point(px(2.0), px(2.0))),
                _ => anchored()
                    .anchor(match self.align {
                        MenuAlign::Start => Anchor::TopLeft,
                        MenuAlign::End => Anchor::TopRight,
                    })
                    .offset(point(px(0.0), px(self.side_offset))),
            };
            let spot = match self.align {
                MenuAlign::Start => div().left_0().bottom_0(),
                MenuAlign::End => div().right_0().bottom_0(),
            };
            spot.absolute().size_0().child(
                deferred(anchored.snap_to_window_with_margin(px(8.0)).child(panel)).priority(2),
            )
        });

        let toggle_state = state.clone();
        let right_state = state.clone();
        div()
            .id(self.id)
            .relative()
            .when(opener == Opener::Click, |wrapper| {
                wrapper.on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    let opening = !toggle_state.read(cx).open;
                    set_open(&toggle_state, opening, None, window, cx);
                })
            })
            .when(opener == Opener::RightClick, |wrapper| {
                wrapper.on_mouse_down(
                    MouseButton::Right,
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        set_open(&right_state, true, Some(event.position), window, cx);
                    },
                )
            })
            .child(trigger)
            .children(popup)
    }
}

fn set_open(
    state: &gpui::Entity<MenuState>,
    open: bool,
    at: Option<Point<Pixels>>,
    window: &mut Window,
    cx: &mut App,
) {
    let (focus, changed) = state.update(cx, |state, cx| {
        let changed = state.open != open;
        state.open = open;
        state.at = at;
        state.highlighted = None;
        state.open_submenu = None;
        cx.notify();
        let focus = open.then(|| state.focus.get_or_insert_with(|| cx.focus_handle()).clone());
        (
            focus,
            changed.then(|| state.on_open_change.clone()).flatten(),
        )
    });
    if let Some(focus) = focus {
        window.focus(&focus, cx);
    }
    if let Some(on_open_change) = changed {
        on_open_change(open, window, cx);
    }
}

/// The selectable rows of a list, for arrow-key movement.
fn selectable(entries: &[MenuEntry]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| match entry {
            MenuEntry::Item(item) => !item.disabled,
            MenuEntry::Submenu { .. } => true,
            MenuEntry::Separator | MenuEntry::Label(_) => false,
        })
        .map(|(index, _)| index)
        .collect()
}

/// `root` is the menu the trigger opened: picking any item closes it.
fn panel(
    root: &gpui::Entity<MenuState>,
    state: &gpui::Entity<MenuState>,
    entries: Vec<MenuEntry>,
    min_width: f32,
    style: ItemStyle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let radius = theme.radius;
    let (highlighted, open_submenu, focus) = {
        let state = state.read(cx);
        (state.highlighted, state.open_submenu, state.focus.clone())
    };
    let rows = selectable(&entries);
    let selects: Vec<Option<OnSelect>> = entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item(item) if !item.disabled => Some(item.on_select.clone()),
            _ => None,
        })
        .collect();
    let key_state = state.clone();
    let key_root = root.clone();
    let out_state = root.clone();
    let mut children: Vec<AnyElement> = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        children.push(match entry {
            MenuEntry::Separator => div()
                .mx(px(-4.0))
                .my(px(4.0))
                .h(px(1.0))
                .bg(colors.border)
                .into_any_element(),
            MenuEntry::Label(label) => div()
                .truncate()
                .px(px(6.0))
                .pt(px(2.0))
                .pb(px(4.0))
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(label)
                .into_any_element(),
            MenuEntry::Item(item) => row(
                root,
                state,
                index,
                highlighted == Some(index),
                item.icon,
                Label {
                    text: item.label,
                    detail: item.detail,
                    trailing: item.trailing,
                },
                false,
                item.disabled,
                item.destructive,
                style,
                Some(item.on_select),
                cx,
            ),
            MenuEntry::Submenu {
                label,
                icon,
                entries,
            } => {
                let open = open_submenu == Some(index);
                let trigger = row(
                    root,
                    state,
                    index,
                    highlighted == Some(index) || open,
                    icon,
                    Label {
                        text: label,
                        detail: None,
                        trailing: None,
                    },
                    true,
                    false,
                    false,
                    style,
                    None,
                    cx,
                );
                let nested = open.then(|| {
                    let sub_state = window.use_keyed_state(
                        ElementId::NamedInteger("menu-sub".into(), index as u64),
                        cx,
                        |_, _| MenuState::default(),
                    );
                    let sub_panel = panel(root, &sub_state, entries, 128.0, style, window, cx);
                    div().absolute().right_0().top_0().size_0().child(
                        deferred(
                            anchored()
                                .anchor(Anchor::TopLeft)
                                .offset(point(px(2.0), px(-5.0)))
                                .snap_to_window_with_margin(px(8.0))
                                .child(sub_panel),
                        )
                        .priority(3),
                    )
                });
                div()
                    .relative()
                    .child(trigger)
                    .children(nested)
                    .into_any_element()
            }
        });
    }
    div()
        .id("menu-panel")
        .relative()
        .when_some(focus, |panel, focus| panel.track_focus(&focus))
        .min_w(px(min_width))
        .p(px(4.0))
        .rounded(radius.lg)
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .shadow(shadow_md(colors.foreground.opacity(0.1)))
        .on_mouse_down_out(move |_, window, cx| {
            if out_state.read(cx).open {
                set_open(&out_state, false, None, window, cx);
            }
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            let key = event.keystroke.key.as_str();
            let current = key_state.read(cx).highlighted;
            let position = current.and_then(|index| rows.iter().position(|row| *row == index));
            let next = match key {
                "escape" => {
                    set_open(&key_root, false, None, window, cx);
                    return;
                }
                "down" => position.map_or(rows.first().copied(), |at| {
                    rows.get((at + 1) % rows.len()).copied()
                }),
                "up" => position.map_or(rows.last().copied(), |at| {
                    rows.get((at + rows.len() - 1) % rows.len()).copied()
                }),
                "right" => {
                    key_state.update(cx, |state, cx| {
                        state.open_submenu = state.highlighted;
                        cx.notify();
                    });
                    return;
                }
                "enter" | "space" => {
                    if let Some(on_select) =
                        current.and_then(|index| selects.get(index).cloned().flatten())
                    {
                        set_open(&key_root, false, None, window, cx);
                        on_select(window, cx);
                    } else {
                        key_state.update(cx, |state, cx| {
                            state.open_submenu = state.highlighted;
                            cx.notify();
                        });
                    }
                    return;
                }
                _ => return,
            };
            cx.stop_propagation();
            key_state.update(cx, |state, cx| {
                state.highlighted = next;
                cx.notify();
            });
        })
        .children(children)
        .child(crate::ui::occlusion::marker())
        .into_any_element()
}
