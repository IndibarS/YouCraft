//! YouCraft's native viewing shell over the shared Crafting Apps engine.
//! Media import/open/seek/save use the engine command channel; playback uses the source clock.

use crate::{
    FilmcraftApp,
    icons::{self, Icon},
    theme::Tokens,
};
use egui::{Align2, Color32, Rect, Sense, pos2, vec2};
use filmcraft_project::ItemId;
use filmcraft_time::Tick;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

mod library;
mod miniplayer;
mod tracks;

pub use library::{load_library, persist, prune_missing_videos, save_library};
pub use tracks::load_subtitles;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerState {
    pub page: String,
    pub query: String,
    pub dislikes: Vec<ItemId>,
    pub miniplayer: bool,
    pub mini_width: Option<f32>,
    pub audio_stream: usize,
    pub history: Vec<ItemId>,
    pub favorites: Vec<ItemId>,
    pub volume: f32,
    pub muted: bool,
    pub theater: bool,
    pub autoplay: bool,
    pub captions: bool,
}
impl Default for PlayerState {
    fn default() -> Self {
        Self {
            page: "library".into(),
            query: String::new(),
            dislikes: vec![],
            miniplayer: false,
            mini_width: None,
            audio_stream: 0,
            history: vec![],
            favorites: vec![],
            volume: 1.0,
            muted: false,
            theater: false,
            autoplay: true,
            captions: true,
        }
    }
}
pub struct Runtime {
    pub enabled: bool,
    pub library_path: Option<String>,
    pub discovery: Option<std::sync::mpsc::Receiver<Result<(String, filmcraft_media::SharedSource), String>>>,
    pub discovery_paused: Arc<std::sync::atomic::AtomicBool>,
    seek_preview: Option<f64>,
    pub gain: Arc<AtomicU32>,
    pub subtitles: filmcraft_captions::Document,
    caption_item: Option<ItemId>,
    was_playing: bool,
    last_activity: f64,
    skip_feedback: Option<(f64, i32)>,
    play_feedback: Option<(f64, bool)>,
    pending_picture_click: Option<f64>,
    tracks_open: bool,
    audio_menu: bool,
    audio_labels: Vec<String>,
    subtitle_labels: Vec<String>,
    selected_subtitle: Option<usize>,
    track_load: Option<std::sync::mpsc::Receiver<Result<Vec<filmcraft_media::SubtitleCue>, String>>>,
    pub thumbnails: Option<crate::frames::FrameServer>,
    pub preview_cache_dir: Option<String>,
    pub thumbnail_failures: std::collections::HashSet<ItemId>,
    storyboard: usize,
    grid_query: String,
    mini_corner: [bool; 2],
    mini_drag_pos: Option<egui::Pos2>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            seek_preview: None,
            library_path: None,
            discovery: None,
            discovery_paused: Arc::default(),
            enabled: false,
            gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            subtitles: Default::default(),
            caption_item: None,
            was_playing: false,
            last_activity: 0.0,
            skip_feedback: None,
            play_feedback: None,
            pending_picture_click: None,
            tracks_open: false,
            audio_menu: false,
            audio_labels: vec![],
            subtitle_labels: vec![],
            selected_subtitle: None,
            track_load: None,
            thumbnails: None,
            preview_cache_dir: None,
            thumbnail_failures: Default::default(),
            storyboard: 0,
            grid_query: String::new(),
            mini_corner: [true, true],
            mini_drag_pos: None,
        }
    }
}

fn volume_icon(state: &PlayerState) -> Icon {
    if state.muted || !state.volume.is_finite() || state.volume <= 0.0 {
        Icon::Mute
    } else if state.volume <= 0.33 {
        Icon::SpeakerLow
    } else if state.volume <= 0.66 {
        Icon::SpeakerMid
    } else {
        Icon::Speaker
    }
}

fn button(app: &mut FilmcraftApp, ui: &mut egui::Ui, id: &str, label: &str) -> bool {
    let icon = match id {
        "player.play" => Some(if label == "Pause" { Icon::Pause } else { Icon::Play }),
        "player.back" | "player.tracks.back" => Some(Icon::ChevronLeft),
        "player.theater" => Some(Icon::Rectangle),
        "player.captions" => Some(Icon::Captions),
        "player.audio" => Some(Icon::Audio),
        "player.mini" | "player.mini.restore" => Some(Icon::Proxy),
        "player.mini.close" => Some(Icon::Close),
        "player.like" => {
            Some(if app.session.state.source_item.is_some_and(|item| app.ui.player.favorites.contains(&item)) { Icon::ThumbUpFilled } else { Icon::ThumbUp })
        }
        "player.dislike" => {
            Some(if app.session.state.source_item.is_some_and(|item| app.ui.player.dislikes.contains(&item)) { Icon::ThumbDownFilled } else { Icon::ThumbDown })
        }
        "player.mute" => Some(volume_icon(&app.ui.player)),
        "player.fullscreen" => Some(Icon::Fullscreen),
        _ => None,
    };
    let r = if let Some(icon) = icon {
        let (rect, response) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
        let fill = if id == "player.play" {
            app.tokens.accent
        } else if response.hovered() {
            app.tokens.row_selected
        } else {
            app.tokens.panel_bg
        };
        ui.painter().rect_filled(rect, 19.0, fill);
        icons::paint(ui.painter(), rect.shrink(10.0), icon, Color32::WHITE);
        response.on_hover_text(label)
    } else {
        let width = if id == "player.seek.back" || id == "player.seek.forward" { 52.0 } else { 0.0 };
        ui.add(egui::Button::new(label).min_size(vec2(width, 32.0)).corner_radius(16))
    };
    app.auto.add(id, r.rect, label);
    r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
fn invoke(app: &mut FilmcraftApp, ctx: &egui::Context, command: &str) {
    if let Err(e) = crate::menus::invoke(app, ctx, command, json!({})) {
        app.ui.status = e;
    }
}
pub fn open(app: &mut FilmcraftApp, item: ItemId) {
    if let Err(error) = prune_missing_videos(app) {
        app.ui.status = error;
        return;
    }
    if app.session.project.item(item).is_none() {
        return;
    }
    app.stop_source();
    app.player.seek_preview = None;
    app.player.pending_picture_click = None;
    app.player.play_feedback = None;
    app.player.skip_feedback = None;
    app.player.storyboard = 0;
    match app.session.execute("source.open", json!({"item":item.0})) {
        Ok(_) => {
            app.ui.player.page = "watch".into();
            app.ui.player.miniplayer = false;
            app.ui.player.audio_stream = 0;
            app.player.tracks_open = false;
            app.player.track_load = None;
            app.player.selected_subtitle = None;
            app.player.audio_labels.clear();
            app.player.subtitle_labels.clear();
            if let Some(source) = app.session.media.source_for(&app.session.project, item, app.session.services.as_ref()) {
                app.player.audio_labels = source.audio_track_labels();
                app.player.subtitle_labels = source.subtitle_tracks();
            }
            app.ui.player.history.retain(|i| *i != item);
            app.ui.player.history.insert(0, item);
            app.ui.player.history.truncate(200);
            app.player.subtitles = Default::default();
            app.player.caption_item = Some(item);
            if let Err(e) = app.play_source() {
                app.ui.status = e;
            }
        }
        Err(e) => app.ui.status = e.to_string(),
    }
}
pub fn seek(app: &mut FilmcraftApp, time: Tick) {
    let playing = app.source_playback.clock.playing;
    app.stop_source();
    if let Err(e) = app.session.execute("source.setPlayhead", json!({"time":time.0})) {
        app.ui.status = e.to_string();
        return;
    }
    if playing && let Err(e) = app.play_source() {
        app.ui.status = e;
    }
}
fn toggle(app: &mut FilmcraftApp) {
    app.player.pending_picture_click = None;
    if let Err(e) = app.toggle_source_play() {
        app.ui.status = e;
    } else {
        app.player.play_feedback = Some((app.now(), app.source_playback.clock.playing));
    }
}
fn skip(app: &mut FilmcraftApp, delta: i32) {
    app.player.pending_picture_click = None;
    let now = app.now();
    let total = match app.player.skip_feedback {
        Some((at, previous)) if now - at < 0.8 && previous.signum() == delta.signum() => previous.saturating_add(delta),
        _ => delta,
    };
    app.player.skip_feedback = Some((now, total));
    seek(app, Tick(app.session.state.source_playhead.0.saturating_add(Tick::from_seconds_f64(f64::from(delta)).0)));
}
fn skip_label(app: &FilmcraftApp, delta: i32) -> String {
    let value = app.player.skip_feedback.filter(|(at, total)| app.now() - at < 0.8 && total.signum() == delta.signum()).map_or(delta, |(_, total)| total);
    format!("{value:+}s")
}

fn fullscreen(ctx: &egui::Context) {
    let on = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!on));
}
fn next(app: &mut FilmcraftApp) {
    let ids = media_ids(app);
    if let Some(current) = app.session.state.source_item
        && let Some(index) = ids.iter().position(|i| *i == current)
        && let Some(id) = ids.get(index + 1)
    {
        let mini = app.ui.player.miniplayer;
        let page = app.ui.player.page.clone();
        open(app, *id);
        if mini {
            app.ui.player.miniplayer = true;
            app.ui.player.page = page;
        }
    }
}
pub fn minimize(app: &mut FilmcraftApp) {
    app.ui.player.miniplayer = app.session.state.source_item.is_some();
    app.ui.player.page = "library".into();
}
pub fn shortcuts(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if ctx.text_edit_focused() {
        return;
    }
    let pressed = |key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    if pressed(egui::Key::Space) || pressed(egui::Key::K) {
        toggle(app);
    }
    if pressed(egui::Key::F) {
        fullscreen(ctx);
    }
    if pressed(egui::Key::M) {
        app.ui.player.muted = !app.ui.player.muted;
    }
    if pressed(egui::Key::T) {
        app.ui.player.theater = !app.ui.player.theater;
    }
    if pressed(egui::Key::C) {
        app.ui.player.captions = !app.ui.player.captions;
    }
    if pressed(egui::Key::I) {
        if app.ui.player.miniplayer {
            app.ui.player.miniplayer = false;
            app.ui.player.page = "watch".into();
        } else {
            minimize(app);
        }
    }
    if pressed(egui::Key::ArrowUp) {
        app.ui.player.volume = (app.ui.player.volume + 0.05).min(2.0);
        app.ui.player.muted = false;
    }
    if pressed(egui::Key::ArrowDown) {
        app.ui.player.volume = (app.ui.player.volume - 0.05).max(0.0);
    }
    if pressed(egui::Key::Escape) {
        app.player.tracks_open = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    }
    let mut delta = 0;
    if pressed(egui::Key::ArrowRight) {
        delta = 5;
    }
    if pressed(egui::Key::ArrowLeft) {
        delta = -5;
    }
    if pressed(egui::Key::L) {
        delta = 10;
    }
    if pressed(egui::Key::J) {
        delta = -10;
    }
    if delta != 0 {
        skip(app, delta);
    }
}
fn timestamp(t: Tick) -> String {
    let seconds = t.seconds().max(0.0) as u64;
    if seconds >= 3600 { format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60) } else { format!("{}:{:02}", seconds / 60, seconds % 60) }
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let t = app.tokens;
    if ctx.input(|i| !i.events.is_empty()) {
        app.player.last_activity = ctx.input(|i| i.time);
    }
    if let Some(result) = app.player.track_load.as_ref().and_then(|rx| rx.try_recv().ok()) {
        app.player.track_load = None;
        match result {
            Ok(cues) => {
                app.player.subtitles = filmcraft_captions::Document {
                    cues: cues
                        .into_iter()
                        .map(|c| filmcraft_captions::Cue { start: c.start, end: c.end, text: c.text, speaker: None, id: None, settings: String::new() })
                        .collect(),
                    ..Default::default()
                };
            }
            Err(e) => app.ui.status = e,
        }
    }
    library::poll_discovery(app);
    precompute_previews(app, &ctx);
    if app.player.discovery.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
    ui.spacing_mut().button_padding = vec2(14.0, 7.0);
    let volume = if app.ui.player.muted || !app.ui.player.volume.is_finite() { 0.0 } else { app.ui.player.volume.clamp(0.0, 2.0) };
    app.player.gain.store(volume.to_bits(), Ordering::Relaxed);
    if app.player.caption_item != app.session.state.source_item {
        app.player.subtitles = Default::default();
        app.player.caption_item = app.session.state.source_item;
    }
    let playing = app.source_playback.clock.playing;
    if app.player.was_playing && !playing && app.ui.player.autoplay {
        let at_end = app
            .session
            .state
            .source_item
            .and_then(|id| filmcraft_engine::clip_ops::source_view(&app.session, id))
            .is_some_and(|v| app.session.state.source_playhead >= Tick(v.end.0.saturating_sub(v.rate.frame_duration().0.saturating_mul(2))));
        if at_end {
            next(app);
        }
    }
    app.player.was_playing = app.source_playback.clock.playing;
    ui.painter().rect_filled(ui.max_rect(), 0.0, t.app_bg);
    if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
        && app.session.state.source_item.is_some()
        && !app.ui.player.miniplayer
        && app.ui.player.page == "watch"
    {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::BLACK).inner_margin(0)).show(ui, |ui| watch(app, ui));
        return;
    }
    egui::Panel::top("youcraft-header").exact_size(72.0).frame(egui::Frame::new().fill(t.app_bg).inner_margin(egui::Margin::symmetric(20, 16))).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                let (mark, _) = ui.allocate_exact_size(vec2(31.0, 31.0), Sense::hover());
                ui.painter().rect_filled(mark, 10.0, t.accent);
                icons::paint(ui.painter(), mark.shrink(7.0), Icon::Play, Color32::WHITE);
                ui.label(egui::RichText::new("YouCraft").size(23.0).strong());
                ui.add_space(30.0);
                let width = (ui.available_width() - 260.0).clamp(100.0, 560.0);
                let r = ui
                    .add_sized(vec2(width, 34.0), egui::TextEdit::singleline(&mut app.ui.player.query).id(egui::Id::unique("player-search-input")).hint_text("Search your videos").margin(vec2(14.0, 8.0)));
                app.auto.add("player.search", r.rect, "Search your videos");
                if r.changed() {
                    minimize(app);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let r = ui.add(
                        egui::Button::new(egui::RichText::new("+  Open videos").color(Color32::WHITE))
                            .fill(t.accent)
                            .min_size(vec2(128.0, 36.0))
                            .corner_radius(18),
                    );
                    app.auto.add("player.import", r.rect, "Open videos");
                    if r.clicked() {
                        invoke(app, &ctx, "file.import");
                        app.ui.player.page = "library".into();
                    }
                    ui.menu_button("Library", |ui| {
                        if button(app, ui, "player.library.save", "Save library…") {
                            library::dialog(app, true);
                            ui.close();
                        }
                        if button(app, ui, "player.library.open", "Open library…") {
                            app.stop_source();
                            library::dialog(app, false);
                            ui.close();
                        }
                        if button(app, ui, "player.demo", "Load demo videos") {
                            invoke(app, &ctx, "file.importDemoFootage");
                            app.ui.player.page = "library".into();
                            ui.close();
                        }
                        ui.separator();
                        ui.label("Space / K   Play or pause\nJ / L   Skip 10 seconds\n← / →   Skip 5 seconds\n↑ / ↓   Volume\nM   Mute     F   Fullscreen\nT   Theater     C   Captions     I   Miniplayer");
                    });
                });
            });
        },
    );
    if !app.ui.status.is_empty() {
        egui::Panel::bottom("youcraft-status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&app.ui.status);
                if ui.small_button("Dismiss").clicked() {
                    app.ui.status.clear();
                }
            });
        });
    }
    let compact = ui.available_width() < 900.0;
    if !app.ui.player.theater || app.ui.player.page != "watch" {
        egui::Panel::left("youcraft-navigation")
            .exact_size(if compact { 82.0 } else { 182.0 })
            .resizable(false)
            .frame(egui::Frame::new().fill(t.app_bg).inner_margin(12))
            .show(ui, |ui| {
                ui.add_space(12.0);
                for (id, label, icon) in [("library", "Library", Icon::Home), ("favorites", "Liked videos", Icon::ThumbUp), ("history", "History", Icon::Clock)]
                {
                    let (r, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
                    response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    app.auto.add(&format!("player.nav.{id}"), r, label);
                    if app.ui.player.page == id || response.hovered() {
                        ui.painter().rect_filled(r, 12.0, t.row_selected);
                    }
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(20.0, 20.0)), icon, t.text);
                    if !compact {
                        ui.painter().text(pos2(r.left() + 43.0, r.center().y), Align2::LEFT_CENTER, label, Tokens::ui(13.0), t.text);
                    }
                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label).clicked() {
                        if id != "watch" {
                            minimize(app);
                        }
                        app.ui.player.page = id.into();
                    }
                    ui.add_space(5.0);
                }
                ui.add_space(28.0);
                ui.separator();
                ui.add_space(16.0);
                ui.label(egui::RichText::new(if compact { "LOCAL" } else { "YOUR VIDEOS.\nYOUR SPACE." }).size(11.0).color(t.text_dim));
                if !compact {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Native playback.\nNo uploads. No ads.").size(12.0).color(t.text_dim));
                }
            });
    }
    let margin = egui::Margin { left: 20, right: 20, top: 20, bottom: if app.ui.player.page == "watch" { 20 } else { 0 } };
    egui::CentralPanel::default().frame(egui::Frame::new().fill(t.app_bg).inner_margin(margin)).show(ui, |ui| {
        if app.ui.player.page == "watch" {
            if app.ui.player.theater {
                egui::ScrollArea::vertical().id_salt("theater-page").show(ui, |ui| watch(app, ui));
            } else {
                watch(app, ui);
            }
        } else {
            library_view(app, ui);
        }
    });
    if app.ui.player.miniplayer && app.ui.player.page != "watch" {
        miniplayer::show(app, &ctx);
    }
}

fn media_ids(app: &FilmcraftApp) -> Vec<ItemId> {
    app.session.project.items.values().filter(|i| i.as_media().is_some_and(|m| m.info.video.is_some())).map(|i| i.id).collect()
}
fn library_view(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let t = app.tokens;
    let title = match app.ui.player.page.as_str() {
        "history" => "Recently watched",
        "favorites" => "Liked videos",
        _ => "Your video library",
    };
    ui.heading(egui::RichText::new(title).size(26.0));
    ui.add_space(6.0);
    let ids = match app.ui.player.page.as_str() {
        "history" => app.ui.player.history.clone(),
        "favorites" => app.ui.player.favorites.clone(),
        _ => media_ids(app),
    };
    let query = app.ui.player.query.to_lowercase();
    let mut matches: Vec<_> = ids
        .into_iter()
        .filter_map(|id| {
            let item = app.session.project.item(id)?;
            crate::video_search::score(&display_name(&item.name), &query).map(|score| (score, id))
        })
        .collect();
    matches.sort_by_key(|(score, _)| *score);
    let ids: Vec<_> = matches.into_iter().map(|(_, id)| id).collect();
    let scan = if app.player.discovery.is_some() { "  ·  Finding videos…" } else { "" };
    ui.label(egui::RichText::new(format!("{} videos  ·  Stored on your device{scan}", ids.len())).color(t.text_dim));
    ui.add_space(24.0);
    if ids.is_empty() {
        ui.add_space(60.0);
        ui.vertical_centered(|ui| {
            ui.heading(if query.is_empty() { "Your videos appear here" } else { "No matching videos" });
            ui.label("Videos in your personal folders are discovered automatically. You can also open any video.");
            ui.add_space(18.0);
            if button(app, ui, "player.empty.import", "Open videos") {
                invoke(app, ui.ctx(), "file.import");
            }
            if button(app, ui, "player.empty.demo", "Try sample videos") {
                invoke(app, ui.ctx(), "file.importDemoFootage");
            }
        });
        return;
    }
    video_grid(app, ui, &ids);
}
fn video_grid(app: &mut FilmcraftApp, ui: &mut egui::Ui, ids: &[ItemId]) {
    let columns = ((ui.available_width() / 300.0).floor() as usize).clamp(1, 5);
    let width = ((ui.available_width() - 24.0 * (columns - 1) as f32 - 14.0) / columns as f32).max(20.0);
    let row_height = width * 0.5625 + 108.0;
    let height = if app.ui.player.page == "watch" { 600.0 } else { ui.available_height().max(1.0) };
    let mut scroll = egui::ScrollArea::vertical();
    if app.player.grid_query != app.ui.player.query {
        app.player.grid_query.clone_from(&app.ui.player.query);
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    let grid = scroll.max_height(height).auto_shrink([false, false]).id_salt("player-video-grid").show_rows(
        ui,
        row_height,
        ids.len().div_ceil(columns),
        |ui, rows| {
            for row in rows {
                ui.horizontal_top(|ui| {
                    ui.set_min_height(row_height);
                    for id in ids.iter().skip(row * columns).take(columns) {
                        ui.allocate_ui_with_layout(vec2(width, row_height - 16.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            card(app, ui, *id, width);
                        });
                        ui.add_space(14.0);
                    }
                });
            }
        },
    );
    app.auto.add("player.grid.viewport", grid.inner_rect, "Video grid");
}

fn card(app: &mut FilmcraftApp, ui: &mut egui::Ui, id: ItemId, width: f32) {
    let Some(item) = app.session.project.item(id) else { return };
    let name = std::path::Path::new(&item.name).file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_else(|| item.name.clone());
    let duration = item.as_media().map(|m| m.info.duration).unwrap_or(Tick::ZERO);
    let t = app.tokens;
    let (r, response) = ui.allocate_exact_size(vec2(width, width * 0.5625), Sense::click());
    response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    app.auto.add(&format!("player.video.{}", id.0), r, &name);
    ui.painter().rect_filled(r, 12.0, t.panel_bg);
    let poster = app.poster(ui.ctx(), id, Tick::from_seconds_f64((duration.seconds() * 0.1).min(30.0)), 320);
    if let Some((texture, size)) = poster {
        paint_thumbnail(ui, r, texture, size, size.x / size.y.max(1.0), 12);
    } else {
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(32.0, 32.0)), Icon::Film, t.text_dim);
    }
    let badge = Rect::from_min_size(r.right_bottom() - vec2(68.0, 27.0), vec2(60.0, 21.0));
    ui.painter().rect_filled(badge, 4.0, Color32::from_black_alpha(210));
    ui.painter().text(badge.center(), Align2::CENTER_CENTER, timestamp(duration), Tokens::ui(11.0), Color32::WHITE);
    if response.hovered() {
        ui.painter().circle_filled(r.center(), 24.0, Color32::from_black_alpha(180));
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(22.0, 22.0)), Icon::Play, Color32::WHITE);
    }
    if poster.is_none()
        && let Some(status) = app.player.thumbnails.as_ref().and_then(|frames| frames.pool.offline_status(id))
    {
        ui.painter().rect_filled(r, 12.0, t.panel_bg);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "Preview unavailable", Tokens::ui(13.0), t.text_dim);
        response.clone().on_hover_text(status.error);
    }
    if poster.is_none() && app.player.thumbnail_failures.contains(&id) {
        ui.painter().rect_filled(r, 12.0, t.panel_bg);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "Preview unavailable", Tokens::ui(13.0), t.text_dim);
    }
    if response.clicked() {
        open(app, id);
    }
    ui.add_space(7.0);
    let title = ui.add(egui::Label::new(egui::RichText::new(&name).strong().size(14.0)).truncate());
    app.auto.add(&format!("player.title.{}", id.0), title.rect, &name);
    ui.label(egui::RichText::new("Local video").size(11.0).color(t.text_dim));
}
fn picture_gestures(app: &mut FilmcraftApp, ctx: &egui::Context, response: &egui::Response, picture: Rect) {
    let now = app.now();
    if response.double_clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            skip(app, if pointer.x < picture.center().x { -5 } else { 5 });
        }
    } else if response.clicked() {
        let direction = response.interact_pointer_pos().map(|p| if p.x < picture.center().x { -1 } else { 1 });
        let continuing = app.player.skip_feedback.is_some_and(|(at, total)| now - at < 0.8 && Some(total.signum()) == direction);
        if continuing {
            skip(app, direction.unwrap_or(1) * 5);
        } else {
            // Wait out the double-click window so seeking never briefly pauses playback.
            app.player.pending_picture_click = Some(now);
        }
    }
    if let Some(at) = app.player.pending_picture_click {
        let delay = ctx.options(|o| o.input_options.max_double_click_delay);
        if now - at >= delay {
            toggle(app);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((delay - (now - at)).max(0.001)));
        }
    }
}

fn playback_feedback(app: &FilmcraftApp, ui: &egui::Ui, picture: Rect) {
    let now = app.now();
    let painter = ui.painter().with_clip_rect(picture.intersect(ui.clip_rect()));
    if let Some((at, delta)) = app.player.skip_feedback
        && now - at < 0.8
    {
        let age = ((now - at) / 0.8).clamp(0.0, 1.0) as f32;
        let center = pos2(picture.center().x + picture.width() * if delta > 0 { 0.28 } else { -0.28 }, picture.center().y);
        painter.circle_filled(center, 48.0 + age * 24.0, Color32::from_black_alpha(((1.0 - age) * 150.0) as u8));
        let direction = if delta > 0 { 1.0 } else { -1.0 };
        for i in 0..3 {
            let x = center.x + (i as f32 - 1.0) * 15.0 * direction;
            let alpha = ((1.0 - age) * (0.55 + 0.45 * (age * 9.0 - i as f32).cos().abs()) * 255.0) as u8;
            painter.add(egui::Shape::convex_polygon(
                vec![pos2(x - 5.0 * direction, center.y - 17.0), pos2(x + 5.0 * direction, center.y - 10.0), pos2(x - 5.0 * direction, center.y - 3.0)],
                Color32::from_white_alpha(alpha),
                egui::Stroke::NONE,
            ));
        }
        painter.text(
            center + vec2(0.0, 15.0),
            Align2::CENTER_CENTER,
            format!("{delta:+} seconds"),
            Tokens::semibold(17.0),
            Color32::from_white_alpha(((1.0 - age) * 255.0) as u8),
        );
        ui.ctx().request_repaint();
    }
    if let Some((at, playing)) = app.player.play_feedback
        && now - at < 0.45
    {
        let age = ((now - at) / 0.45).clamp(0.0, 1.0) as f32;
        let radius = 30.0 + age * 15.0;
        painter.circle_filled(picture.center(), radius, Color32::from_black_alpha(((1.0 - age) * 170.0) as u8));
        let rect = Rect::from_center_size(picture.center(), vec2(radius, radius));
        icons::paint(&painter, rect, if playing { Icon::Play } else { Icon::Pause }, Color32::from_white_alpha(((1.0 - age) * 255.0) as u8));
        ui.ctx().request_repaint();
    }
}

fn watch(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let Some(id) = app.session.state.source_item else {
        ui.heading("Choose a video from your library");
        return;
    };
    let title = app.session.project.item(id).map(|i| display_name(&i.name)).unwrap_or_default();
    let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
    let wide = ui.available_width() > 850.0 && !app.ui.player.theater && !full;
    if wide {
        egui::Panel::right("youcraft-videos")
            .exact_size(300.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(app.tokens.app_bg).inner_margin(egui::Margin { left: 20, right: 0, top: 0, bottom: 0 }))
            .show(ui, |ui| video_list(app, ui));
    }
    if !full && button(app, ui, "player.back", "Back to library") {
        minimize(app);
    }
    let aspect = video_aspect(app, id);
    let max_height = if full {
        ui.available_height()
    } else if app.ui.player.theater {
        ui.available_width() / aspect.max(0.1)
    } else {
        (ui.available_height() - 116.0).max(120.0)
    };
    let size = fitted_video_size(ui.available_width(), max_height, aspect);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let picture = Rect::from_center_size(area.center(), size);
    let response = ui.interact(picture, ui.scope_id().with("video-picture"), Sense::click());
    response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    crate::panels::monitor::player_picture(app, ui, picture);
    picture_gestures(app, ui.ctx(), &response, picture);
    let now = app.now();
    let visible = !app.source_playback.clock.playing || now - app.player.last_activity < 2.5 || app.player.tracks_open || app.player.seek_preview.is_some();
    if app.ui.player.captions {
        let time = app.session.state.source_playhead;
        let text = app.player.subtitles.cues.iter().filter(|c| c.start <= time && time < c.end).map(|c| c.text.as_str()).collect::<Vec<_>>().join("\n");
        if !text.is_empty() {
            let galley = ui.painter().layout(text, Tokens::semibold(22.0), Color32::WHITE, (picture.width() - 48.0).max(1.0));
            let r = Rect::from_center_size(pos2(picture.center().x, picture.bottom() - if visible { 114.0 } else { 40.0 }), galley.size() + vec2(22.0, 12.0));
            ui.painter().rect_filled(r, 5.0, Color32::from_black_alpha(210));
            ui.painter().galley(r.min + vec2(11.0, 6.0), galley, Color32::WHITE);
        }
    }
    playback_feedback(app, ui, picture);
    if visible {
        let controls = Rect::from_min_max(pos2(picture.left(), (picture.bottom() - 96.0).max(picture.top())), picture.max);
        ui.painter().rect_filled(controls, 0.0, Color32::from_black_alpha(205));
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls.shrink2(vec2(12.0, 6.0))), |ui| controls_ui(app, ui, id, picture));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    }
    tracks::show(app, ui.ctx());
    if full {
        return;
    }
    ui.add_space(12.0);
    ui.add(egui::Label::new(egui::RichText::new(title).size(21.0).strong()).truncate());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("From your local library").color(app.tokens.text_dim));
        let liked = app.ui.player.favorites.contains(&id);
        let disliked = app.ui.player.dislikes.contains(&id);
        if button(app, ui, "player.like", if liked { "Unlike" } else { "Like" }) {
            app.ui.player.favorites.retain(|i| *i != id);
            if !liked {
                app.ui.player.favorites.push(id);
            }
            app.ui.player.dislikes.retain(|i| *i != id);
        }
        if button(app, ui, "player.dislike", if disliked { "Remove dislike" } else { "Dislike" }) {
            app.ui.player.dislikes.retain(|i| *i != id);
            if !disliked {
                app.ui.player.dislikes.push(id);
            }
            app.ui.player.favorites.retain(|i| *i != id);
        }
        let r = ui.checkbox(&mut app.ui.player.autoplay, "Autoplay");
        app.auto.add("player.autoplay", r.rect, "Autoplay following videos");
        let r = ui.checkbox(&mut app.source_playback.clock.looping, "Loop");
        app.auto.add("player.loop", r.rect, "Loop video");
    });
    if app.ui.player.theater {
        ui.add_space(16.0);
        let heading = ui.strong("More videos");
        app.auto.add("player.theater.grid", heading.rect, "More videos below theater");
        let ids: Vec<_> = media_ids(app).into_iter().filter(|other| *other != id).collect();
        video_grid(app, ui, &ids);
    } else if !wide {
        video_list(app, ui);
    }
}

/// Draw the seek rail ourselves so hover fill and the solid playhead share exact endpoints.
fn seek_bar(ui: &mut egui::Ui, seconds: &mut f64, start: f64, end: f64) -> egui::Response {
    seek_bar_height(ui, seconds, start, end, 24.0)
}

fn seek_bar_height(ui: &mut egui::Ui, seconds: &mut f64, start: f64, end: f64, height: f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(ui.available_width().max(1.0), height), Sense::click_and_drag());
    response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let duration = (end - start).max(0.0);
    let fraction_at = |x: f32| ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    if (response.clicked() || response.dragged() || response.drag_stopped())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        *seconds = start + f64::from(fraction_at(pointer.x)) * duration;
        response.mark_changed();
    }
    let hovering = response.hovered() || response.dragged();
    let height = if hovering { 6.0 } else { 4.0 };
    let rail = Rect::from_center_size(rect.center(), vec2(rect.width(), height));
    let painter = ui.painter();
    painter.rect_filled(rail, 0.0, Color32::from_gray(48));
    if hovering && let Some(pointer) = ui.ctx().pointer_latest_pos() {
        let hover_x = pointer.x.clamp(rect.left(), rect.right());
        painter.rect_filled(Rect::from_min_max(rail.min, pos2(hover_x, rail.bottom())), 0.0, Color32::from_gray(145));
    }
    let fraction = if duration > 0.0 { ((*seconds - start) / duration).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let playhead = rect.left() + fraction * rect.width();
    let red = Color32::from_rgb(255, 0, 51);
    painter.rect_filled(Rect::from_min_max(rail.min, pos2(playhead, rail.bottom())), 0.0, red);
    let hover_amount = ui.ctx().animate_bool_with_time(response.id.with("seek-dot-hover"), hovering, 0.12);
    painter.circle_filled(pos2(playhead, rail.center().y), 6.0 + 3.0 * hover_amount, red);
    response
}

fn seek_hover_preview(app: &mut FilmcraftApp, ui: &egui::Ui, id: ItemId, range: (f64, f64), picture: Rect, response: &egui::Response) {
    if (response.hovered() || response.dragged())
        && let Some(pointer) = response.hover_pos().or_else(|| ui.ctx().pointer_latest_pos())
    {
        let fraction = ((pointer.x - response.rect.left()) / response.rect.width().max(1.0)).clamp(0.0, 1.0);
        let time = range.0 + f64::from(fraction) * (range.1 - range.0);
        let preview_time = (time / preview_step(range.1)).floor() * preview_step(range.1);
        let preview = Rect::from_min_size(
            pos2(pointer.x.clamp(picture.left() + 90.0, picture.right().max(picture.left() + 180.0) - 90.0) - 80.0, response.rect.top() - 120.0),
            vec2(160.0, 112.0),
        );
        app.auto.add("player.seek.preview", preview, "Seek thumbnail preview");
        ui.painter().rect_filled(preview.expand(4.0), 7.0, Color32::from_gray(20));
        {
            if let Some((texture, size)) = app.thumbnail(ui.ctx(), id, Tick::from_seconds_f64(preview_time), 160) {
                paint_thumbnail(ui, Rect::from_min_size(preview.min, vec2(160.0, 90.0)), texture, size, video_aspect(app, id), 4);
            } else {
                ui.painter().text(preview.center(), Align2::CENTER_CENTER, "Loading preview…", Tokens::ui(12.0), Color32::WHITE);
            }
        }
        ui.painter().text(
            pos2(preview.center().x, preview.bottom() - 10.0),
            Align2::CENTER_CENTER,
            timestamp(Tick::from_seconds_f64(time)),
            Tokens::ui(12.0),
            Color32::WHITE,
        );
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(160));
    }
}

fn controls_ui(app: &mut FilmcraftApp, ui: &mut egui::Ui, id: ItemId, picture: Rect) {
    let Some(view) = filmcraft_engine::clip_ops::source_view(&app.session, id) else { return };
    let end = view.end.seconds().max(view.start.seconds());
    let mut seconds = app.player.seek_preview.unwrap_or_else(|| app.session.state.source_playhead.seconds());
    let response = seek_bar(ui, &mut seconds, view.start.seconds(), end);
    app.auto.add("player.seek", response.rect, "Seek video");
    if response.dragged() {
        app.player.seek_preview = Some(seconds);
    }
    if response.drag_stopped() || (response.changed() && !response.dragged()) {
        app.player.seek_preview = None;
        seek(app, Tick::from_seconds_f64(seconds));
    }
    seek_hover_preview(app, ui, id, (view.start.seconds(), end), picture, &response);
    ui.horizontal(|ui| {
        let back_label = skip_label(app, -5);
        if button(app, ui, "player.seek.back", &back_label) {
            skip(app, -5);
        }
        if button(app, ui, "player.play", if app.source_playback.clock.playing { "Pause" } else { "Play" }) {
            toggle(app);
        }
        let forward_label = skip_label(app, 5);
        if button(app, ui, "player.seek.forward", &forward_label) {
            skip(app, 5);
        }
        if button(app, ui, "player.mute", if app.ui.player.muted { "Unmute" } else { "Mute" }) {
            app.ui.player.muted = !app.ui.player.muted;
        }
        if ui.available_width() > 420.0 {
            ui.spacing_mut().slider_width = 70.0;
            let r = ui.add(egui::Slider::new(&mut app.ui.player.volume, 0.0..=2.0).show_value(false).handle_shape(egui::style::HandleShape::Circle));
            let label = format!("Volume: {:.0}%", app.ui.player.volume * 100.0);
            app.auto.add("player.volume", r.rect, &label);
            let cursor = if r.dragged() { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::PointingHand };
            r.on_hover_and_drag_cursor(cursor).on_hover_text(&label);
            ui.label(format!("{:.0}%", app.ui.player.volume * 100.0));
        }
        ui.label(format!("{} / {}", timestamp(app.session.state.source_playhead), timestamp(view.end)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button(app, ui, "player.fullscreen", "Fullscreen (F)") {
                fullscreen(ui.ctx());
            }
            if button(app, ui, "player.theater", "Theater (T)") {
                app.ui.player.theater = !app.ui.player.theater;
            }
            if button(app, ui, "player.mini", "Miniplayer (I)") {
                minimize(app);
            }
            if button(app, ui, "player.captions", "Subtitles/CC") {
                app.player.tracks_open = !app.player.tracks_open || app.player.audio_menu;
                app.player.audio_menu = false;
            }
            if button(app, ui, "player.audio", "Audio track") {
                app.player.tracks_open = !app.player.tracks_open || !app.player.audio_menu;
                app.player.audio_menu = true;
            }
        });
    });
}

/// Matching track menus share sizing, row styling and scrolling.
fn video_list(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    ui.strong("More videos");
    ui.add_space(10.0);
    let ids: Vec<_> = media_ids(app).into_iter().filter(|id| Some(*id) != app.session.state.source_item).collect();
    egui::ScrollArea::vertical().id_salt("related-videos").show_rows(ui, 88.0, ids.len(), |ui, rows| {
        for index in rows {
            let Some(id) = ids.get(index).copied() else { continue };
            let name = app.session.project.item(id).map(|i| display_name(&i.name)).unwrap_or_default();
            let duration = app.session.project.item(id).and_then(|i| i.as_media()).map_or(Tick::ZERO, |m| m.info.duration);
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 80.0), Sense::click());
            response.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
            app.auto.add(&format!("player.related.{}", id.0), rect, &name);
            let thumb = Rect::from_min_size(rect.min, vec2(120.0, 68.0));
            ui.painter().rect_filled(thumb, 7.0, app.tokens.panel_bg);
            if let Some((texture, size)) = app.poster(ui.ctx(), id, Tick::from_seconds_f64((duration.seconds() * 0.1).min(30.0)), 160) {
                paint_thumbnail(ui, thumb, texture, size, size.x / size.y.max(1.0), 7);
            }
            let galley = ui.painter().layout(name, Tokens::semibold(12.0), app.tokens.text, (rect.width() - 132.0).max(1.0));
            ui.painter().with_clip_rect(rect).galley(rect.min + vec2(132.0, 4.0), galley, app.tokens.text);
            if response.clicked() {
                open(app, id);
            }
        }
    });
}
fn display_name(name: &str) -> String {
    std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_else(|| name.to_string())
}

pub fn fitted_video_size(width: f32, height: f32, aspect: f32) -> egui::Vec2 {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect.clamp(0.1, 10.0) } else { 16.0 / 9.0 };
    let width = width.max(1.0).min(height.max(1.0) * aspect);
    vec2(width, width / aspect)
}
fn video_aspect(app: &FilmcraftApp, id: ItemId) -> f32 {
    app.session
        .project
        .item(id)
        .and_then(|i| i.as_media())
        .and_then(|m| m.info.video.as_ref())
        .map_or(16.0 / 9.0, |v| v.width.max(1) as f32 * v.par.0.max(1) as f32 / (v.height.max(1) as f32 * v.par.1.max(1) as f32))
}

fn preview_step(duration: f64) -> f64 {
    ((duration / 128.0 / 2.0).ceil().max(1.0) * 2.0).max(2.0)
}
fn precompute_previews(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let Some(item) = app.session.state.source_item else { return };
    if app.player.storyboard >= 128 {
        return;
    }
    // Start at the beginning, midpoint, quarters, etc., then fill the gaps. One queued job at a
    // time prevents precomputation from holding up grid or hover requests.
    let Some(media) = app.session.project.item(item).and_then(|i| i.as_media()) else { return };
    let Some(video) = &media.info.video else { return };
    let duration = media.info.duration.seconds();
    let source_width = video.width.max(1);
    let rate = video.frame_rate;
    let sample = (app.player.storyboard as u8).reverse_bits() >> 1;
    let seconds = f64::from(sample) * preview_step(duration);
    if seconds >= duration {
        app.player.storyboard += 1;
        ctx.request_repaint();
        return;
    }
    if app.player.thumbnails.is_none() {
        let _ = app.thumbnail(ctx, item, Tick::ZERO, 160);
    }
    let Some(frames) = &app.player.thumbnails else { return };
    if frames.queue_len() > 0 {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        return;
    }
    let frame = rate.frame_at(Tick::from_seconds_f64(seconds));
    let key = crate::frames::FrameKey { target: crate::frames::Target::Item(item), frame, size: 160, revision: app.item_revision(item), draft: false };
    frames.request(key, rate.tick_of(frame), 160.0 / source_width as f32, &app.session.project, 60);
    app.player.storyboard += 1;
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

/// Fill a tile by cropping its edges, preserving the video's display aspect (including PAR).
pub fn thumbnail_uv(aspect: f32, tile_aspect: f32) -> Rect {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
    let tile_aspect = if tile_aspect.is_finite() && tile_aspect > 0.0 { tile_aspect } else { 1.0 };
    let size = if aspect > tile_aspect { vec2(tile_aspect / aspect, 1.0) } else { vec2(1.0, aspect / tile_aspect) };
    Rect::from_center_size(pos2(0.5, 0.5), size)
}
fn paint_thumbnail(ui: &egui::Ui, rect: Rect, texture: egui::TextureId, size: egui::Vec2, aspect: f32, radius: u8) {
    egui::Image::new((texture, size)).uv(thumbnail_uv(aspect, rect.width() / rect.height().max(1.0))).corner_radius(radius).paint_at(ui, rect);
}
