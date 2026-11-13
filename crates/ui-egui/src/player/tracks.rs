//! Audio-track selection and embedded/external subtitle loading.

use super::{FilmcraftApp, button};
use crate::icons::{self, Icon};
use egui::{Align2, Color32, Sense, vec2};

fn track_choice(app: &mut FilmcraftApp, ui: &mut egui::Ui, id: &str, label: &str, selected: bool) -> bool {
    let text = format!("{}  {label}", if selected { "✓" } else { "   " });
    let r = ui.add(egui::Button::new("").left_text(text).truncate().frame_when_inactive(false).min_size(vec2(ui.available_width(), 42.0)));
    app.auto.add(id, r.rect, label);
    r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(label).clicked()
}

fn audio_choices(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    if track_choice(app, ui, "player.audio.off", "Off", app.ui.player.muted) {
        app.ui.player.muted = true;
        app.player.tracks_open = false;
    }
    for (index, label) in app.player.audio_labels.clone().iter().enumerate() {
        let selected = !app.ui.player.muted && app.ui.player.audio_stream == index;
        if track_choice(app, ui, &format!("player.audio.{index}"), label, selected) {
            let playing = app.source_playback.clock.playing;
            app.stop_source();
            app.ui.player.audio_stream = index;
            app.ui.player.muted = false;
            if playing && let Err(e) = app.play_source() {
                app.ui.status = e;
            }
            app.player.tracks_open = false;
        }
    }
    if app.player.audio_labels.is_empty() {
        ui.label("No audio tracks available");
    }
}

fn caption_choices(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let off = !app.ui.player.captions || (app.player.selected_subtitle.is_none() && app.player.subtitles.cues.is_empty());
    if track_choice(app, ui, "player.subtitle.off", "Off", off) {
        app.ui.player.captions = false;
        app.player.tracks_open = false;
    }
    for (index, label) in app.player.subtitle_labels.clone().iter().enumerate() {
        let selected = app.ui.player.captions && app.player.selected_subtitle == Some(index);
        if track_choice(app, ui, &format!("player.subtitle.{index}"), label, selected) {
            select_subtitle(app, index);
            app.player.tracks_open = false;
        }
    }
    if app.player.selected_subtitle.is_none()
        && !app.player.subtitles.cues.is_empty()
        && track_choice(app, ui, "player.subtitle.external", "External subtitles", app.ui.player.captions)
    {
        app.ui.player.captions = true;
        app.player.tracks_open = false;
    }
    if app.player.track_load.is_some() {
        ui.label("Loading subtitles…");
    }
    ui.separator();
    if track_choice(app, ui, "player.captions.load", "Open subtitle file…", false) {
        app.player.tracks_open = false;
        let result = app.pick(crate::native_dialogs::FileDialog::open_files(&["srt", "vtt"]), |app, paths| {
            if let Some(path) = paths.first() {
                load_subtitles(app, path)?;
            }
            Ok(serde_json::Value::Null)
        });
        if let Err(e) = result {
            app.ui.status = e;
        }
    }
}

pub(super) fn show(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if !app.player.tracks_open {
        return;
    }
    let audio = app.player.audio_menu;
    let title = if audio { "Audio track" } else { "Subtitles/CC" };
    egui::Window::new(title)
        .id(egui::Id::unique("player-tracks"))
        .title_bar(false)
        .resizable(false)
        .fixed_size(vec2(310.0, 330.0))
        .anchor(Align2::RIGHT_CENTER, vec2(-28.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_size(vec2(310.0, 330.0));
            ui.horizontal(|ui| {
                if button(app, ui, "player.tracks.back", "Close track menu") {
                    app.player.tracks_open = false;
                }
                let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                icons::paint(ui.painter(), rect, if audio { Icon::Audio } else { Icon::Captions }, Color32::WHITE);
                ui.strong(title);
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt(if audio { "audio-tracks" } else { "subtitle-tracks" }).max_height(270.0).show(ui, |ui| {
                if audio {
                    audio_choices(app, ui);
                } else {
                    caption_choices(app, ui);
                }
            });
        });
}
fn select_subtitle(app: &mut FilmcraftApp, index: usize) {
    if app.player.track_load.is_some() {
        return;
    }
    let Some(id) = app.session.state.source_item else { return };
    let Some(source) = app.session.media.source_for(&app.session.project, id, app.session.services.as_ref()) else { return };
    app.player.selected_subtitle = Some(index);
    app.ui.player.captions = true;
    app.player.subtitles = Default::default();
    let (tx, rx) = std::sync::mpsc::channel();
    app.player.track_load = Some(rx);
    let result = std::thread::Builder::new().name("youcraft-subtitles".into()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.subtitle_cues(index)))
            .map_err(|_| "Subtitle reader failed".to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
        let _ = tx.send(result);
    });
    if let Err(e) = result {
        app.player.track_load = None;
        app.ui.status = e.to_string();
    }
}

pub fn load_subtitles(app: &mut FilmcraftApp, path: &str) -> Result<(), String> {
    if app.session.services.file_size(path).map_err(|e| e.to_string())? > 4 * 1024 * 1024 {
        return Err("Subtitle files must be smaller than 4 MB".into());
    }
    let bytes = app.session.services.read_range(path, 0, 4 * 1024 * 1024 + 1).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Subtitle files must be smaller than 4 MB".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "Subtitles must use UTF-8 text".to_string())?;
    let doc =
        if text.trim_start_matches('\u{feff}').starts_with("WEBVTT") { filmcraft_captions::vtt::parse(&text) } else { filmcraft_captions::srt::parse(&text) }
            .map_err(|e| e.to_string())?;
    app.ui.status = format!("Loaded {} subtitle cues", doc.cues.len());
    app.player.track_load = None;
    app.player.selected_subtitle = None;
    app.ui.player.captions = true;
    app.player.subtitles = doc;
    app.player.caption_item = app.session.state.source_item;
    Ok(())
}
