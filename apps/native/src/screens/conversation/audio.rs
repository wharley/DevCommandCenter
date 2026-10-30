//! `AudioAttachment`: an attached audio file plays in place like a voice
//! note: play/pause, its waveform (click or drag to seek) and its length,
//! the time played once started. The file chip when it is gone or cannot be
//! decoded. One clip plays at a time. Each card is its own view, made once
//! per file, so it keeps its place and playback across redraws.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Bounds, Context, Entity, Global, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    Task, WeakEntity, Window, canvas, div, prelude::*, px,
};

use super::user_row::attachment_chip;
use crate::data::audio_clip::{self, BARS, Clip};
use crate::data::prompt::basename;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// How often the time and the played bars follow playback.
const FOLLOW: Duration = Duration::from_millis(100);

enum Load {
    Loading,
    Ready(Clip),
    Failed,
}

pub struct AudioCard {
    path: String,
    load: Load,
    playing: bool,
    /// Seconds played, or where playback will start.
    position: f64,
    #[cfg(target_os = "macos")]
    player: Option<objc2::rc::Retained<objc2_avf_audio::AVAudioPlayer>>,
    /// The waveform's bounds as painted, for seeking.
    track: Rc<Cell<Bounds<Pixels>>>,
    _follow: Option<Task<()>>,
}

#[derive(Default)]
struct Cards(HashMap<String, Entity<AudioCard>>);

impl Global for Cards {}

/// The clip playing now; starting another pauses it.
struct NowPlaying(WeakEntity<AudioCard>);

impl Global for NowPlaying {}

/// The card for `path`, made the first time it shows.
pub(super) fn card(path: &str, cx: &mut App) -> Entity<AudioCard> {
    if let Some(card) = cx.default_global::<Cards>().0.get(path) {
        return card.clone();
    }
    let card = cx.new(|cx| AudioCard::new(path.to_string(), cx));
    cx.global_mut::<Cards>()
        .0
        .insert(path.to_string(), card.clone());
    card
}

#[cfg(target_os = "macos")]
fn open(path: &str) -> Option<objc2::rc::Retained<objc2_avf_audio::AVAudioPlayer>> {
    use objc2::AllocAnyThread;
    use objc2_avf_audio::AVAudioPlayer;
    use objc2_foundation::{NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    // SAFETY: a player for a file URL; it lives on the UI thread with the card.
    unsafe {
        let player =
            AVAudioPlayer::initWithContentsOfURL_error(AVAudioPlayer::alloc(), &url).ok()?;
        player.prepareToPlay();
        Some(player)
    }
}

impl AudioCard {
    fn new(path: String, cx: &mut Context<Self>) -> Self {
        let target = PathBuf::from(&path);
        cx.spawn(async move |this, cx| {
            let clip = cx
                .background_executor()
                .spawn(async move { audio_clip::read(&target) })
                .await;
            let _ = this.update(cx, |card, cx| {
                card.load = clip.map_or(Load::Failed, Load::Ready);
                cx.notify();
            });
        })
        .detach();
        Self {
            path,
            load: Load::Loading,
            playing: false,
            position: 0.0,
            #[cfg(target_os = "macos")]
            player: None,
            track: Rc::new(Cell::new(Bounds::default())),
            _follow: None,
        }
    }

    fn duration(&self) -> f64 {
        match &self.load {
            Load::Ready(clip) => clip.duration,
            _ => 0.0,
        }
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.playing {
            self.pause(cx);
        } else {
            self.play(cx);
        }
    }

    #[cfg(target_os = "macos")]
    fn play(&mut self, cx: &mut Context<Self>) {
        let me = cx.entity();
        if let Some(other) = cx
            .try_global::<NowPlaying>()
            .and_then(|now| now.0.upgrade())
            .filter(|other| *other != me)
        {
            other.update(cx, |other, cx| other.pause(cx));
        }
        if self.player.is_none() {
            self.player = open(&self.path);
        }
        let Some(player) = self.player.as_ref() else {
            self.load = Load::Failed;
            cx.notify();
            return;
        };
        if self.position >= self.duration() {
            self.position = 0.0;
        }
        // SAFETY: the card's own player, on the UI thread.
        unsafe {
            player.setCurrentTime(self.position);
            player.play();
        }
        self.playing = true;
        cx.set_global(NowPlaying(me.downgrade()));
        self._follow = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FOLLOW).await;
                if !this.update(cx, |card, cx| card.follow(cx)).unwrap_or(false) {
                    break;
                }
            }
        }));
        cx.notify();
    }

    #[cfg(not(target_os = "macos"))]
    fn play(&mut self, _: &mut Context<Self>) {}

    /// Keeps up with playback; false once it ended.
    fn follow(&mut self, cx: &mut Context<Self>) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(player) = self.player.as_ref() {
            // SAFETY: the card's own player, on the UI thread.
            let (playing, time) = unsafe { (player.isPlaying(), player.currentTime()) };
            if playing {
                self.position = time;
            } else {
                self.playing = false;
                self.position = 0.0;
                self._follow = None;
            }
            cx.notify();
            return playing;
        }
        let _ = cx;
        false
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(player) = self.player.as_ref() {
            // SAFETY: the card's own player, on the UI thread.
            unsafe {
                self.position = player.currentTime();
                player.pause();
            }
        }
        self.playing = false;
        self._follow = None;
        cx.notify();
    }

    fn seek_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let track = self.track.get();
        if track.size.width <= px(0.0) {
            return;
        }
        let ratio = ((x - track.left()) / track.size.width).clamp(0.0, 1.0);
        self.position = f64::from(ratio) * self.duration();
        #[cfg(target_os = "macos")]
        if let Some(player) = self.player.as_ref() {
            // SAFETY: the card's own player, on the UI thread.
            unsafe { player.setCurrentTime(self.position) };
        }
        cx.notify();
    }
}

/// `Bars`: one bar per slice, a gentle curve so speech and pauses read apart.
fn bars(levels: &[f32], progress: f32, cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    let (played, rest) = (
        colors.foreground.opacity(0.8),
        colors.muted_foreground.opacity(0.35),
    );
    let count = levels.len().max(1) as f32;
    div()
        .flex()
        .h(px(24.0))
        .items_center()
        .justify_between()
        .children(levels.iter().enumerate().map(|(index, level)| {
            let percent = (level.powf(1.5) * 100.0).round().max(12.0);
            div()
                .w(px(2.0))
                .h(px(24.0 * percent / 100.0))
                .rounded_full()
                .bg(if (index as f32 + 0.5) / count <= progress {
                    played
                } else {
                    rest
                })
        }))
}

impl Render for AudioCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let radius = cx.theme().radius;
        let (levels, progress, time) = match &self.load {
            Load::Failed => return attachment_chip(&self.path, cx),
            Load::Loading => (vec![0.2; BARS], 0.0, None),
            Load::Ready(clip) => (
                clip.peaks.clone(),
                if clip.duration > 0.0 {
                    (self.position / clip.duration) as f32
                } else {
                    0.0
                },
                Some(audio_clip::format_time(if self.position > 0.0 {
                    self.position
                } else {
                    clip.duration
                })),
            ),
        };
        let ready = matches!(self.load, Load::Ready(_));
        let name = basename(&self.path);
        let label = match name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => {
                format!("{stem} · {}", extension.to_uppercase())
            }
            _ => name.to_string(),
        };
        let button = div()
            .id("audio-play")
            .flex()
            .size(px(32.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded_full()
            .map(|button| {
                if ready {
                    button
                        .bg(colors.foreground)
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.85))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|card, _, _, cx| card.toggle(cx)))
                        .child(
                            div().when(!self.playing, |icon| icon.ml(px(2.0))).child(
                                Icon::new(if self.playing { "pause" } else { "play" })
                                    .size(14.0)
                                    .stroke(2.6)
                                    .color(colors.background),
                            ),
                        )
                } else {
                    button.bg(colors.muted)
                }
            });
        let track_bounds = self.track.clone();
        let track = div()
            .relative()
            .when(ready, |track| {
                track
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|card, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            card.seek_to(event.position.x, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|card, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left)
                            && card.track.get().contains(&event.position)
                        {
                            card.seek_to(event.position.x, cx);
                        }
                    }))
            })
            .child(
                canvas(
                    move |bounds, _, _| track_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(bars(&levels, progress, cx));
        div()
            .debug_selector(|| "user-message-audio".into())
            .flex()
            .items_center()
            .gap(px(10.0))
            .w(px(304.0))
            .max_w_full()
            .rounded(radius.xl)
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.background.opacity(0.7))
            .py(px(8.0))
            .pr(px(12.0))
            .pl(px(8.0))
            .child(button)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .flex_1()
                    .min_w_0()
                    .child(track)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(colors.muted_foreground)
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .children(time.map(|time| div().flex_shrink_0().child(time))),
                    ),
            )
            .into_any_element()
    }
}
