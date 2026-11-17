use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_time::Tick;
use filmcraft_ui_egui::{FilmcraftApp, player};
use serde_json::json;

fn app() -> FilmcraftApp {
    let mut session = Session::default();
    session.execute("file.importDemoFootage", json!({})).unwrap();
    let mut app = FilmcraftApp::new(session);
    app.player.enabled = true;
    app
}
fn harness(width: f32) -> Harness<'static, FilmcraftApp> {
    let app = app();
    let mut builder = Harness::builder().with_size(egui::vec2(width, 900.0)).with_max_steps(10000);
    if std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR").is_some() {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(move |_| app);
    for _ in 0..4 {
        h.step();
    }
    h
}
#[test]
fn player_shell_has_media_without_editing_controls() {
    for width in [900.0, 1600.0] {
        let h = harness(width);
        assert!(h.state().ui_error.is_none(), "{:?}", h.state().ui_error);
        assert!(!h.state().auto.query("player.video.").is_empty());
        for card in h.state().auto.query("player.video.") {
            assert!(card.rect[0] + card.rect[2] <= width + 1.0, "card overflows viewport: {:?}", card.rect);
            let title_id = card.id.replace("player.video.", "player.title.");
            let title = h.state().auto.find(&title_id).unwrap();
            assert!(title.rect[1] >= card.rect[1] + card.rect[3], "title must be below thumbnail");
        }
        assert!(h.state().auto.query("timeline.").is_empty());
        assert!(h.state().auto.query("header.mode.export").is_empty());
    }
}
#[test]
fn playback_and_seek_do_not_edit_the_library() {
    let mut app = app();
    let before = serde_json::to_value(&app.session.project).unwrap();
    let id = *app.session.project.items.keys().next().unwrap();
    player::open(&mut app, id);
    assert!(app.source_playback.clock.playing);
    app.stop_source();
    let view = filmcraft_engine::clip_ops::source_view(&app.session, id).unwrap();
    let target = view.rate.snap(Tick::from_seconds_f64(1.0));
    player::seek(&mut app, target);
    assert_eq!(app.session.state.source_playhead, target);
    assert_eq!(serde_json::to_value(&app.session.project).unwrap(), before);
    assert_eq!(app.ui.player.history, vec![id]);
}
#[test]
fn search_filters_visible_cards_and_volume_reaches_audio_control() {
    let mut h = harness(1600.0);
    h.state_mut().ui.player.query = "no-such-video-name".into();
    h.state_mut().ui.player.volume = 0.25;
    for _ in 0..3 {
        h.step();
    }
    assert!(h.state().auto.query("player.video.").is_empty());
    assert_eq!(f32::from_bits(h.state().player.gain.load(std::sync::atomic::Ordering::Relaxed)), 0.25);
    h.state_mut().ui.player.volume = 2.0;
    h.step();
    assert_eq!(f32::from_bits(h.state().player.gain.load(std::sync::atomic::Ordering::Relaxed)), 2.0);
    h.state_mut().ui.player.muted = true;
    h.step();
    assert_eq!(f32::from_bits(h.state().player.gain.load(std::sync::atomic::Ordering::Relaxed)), 0.0);
}
#[test]
fn loads_subtitle_timing_and_rejects_invalid_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("captions.srt");
    std::fs::write(&path, "1\n00:00:01,000 --> 00:00:03,000\nHello YouCraft\n").unwrap();
    let mut app = app();
    player::load_subtitles(&mut app, path.to_str().unwrap()).unwrap();
    assert_eq!(app.player.subtitles.cues.len(), 1);
    assert_eq!(app.player.subtitles.cues[0].start, Tick::from_seconds_f64(1.0));
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(player::load_subtitles(&mut app, path.to_str().unwrap()).is_err());
    assert_eq!(app.player.subtitles.cues.len(), 1, "failed load preserves current subtitles");
}

#[test]
fn saved_library_restores_dislikes_likes_and_volume() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.fcproj");
    let mut first = app();
    let id = *first.session.project.items.keys().next().unwrap();
    first.ui.player.dislikes.push(id);
    first.ui.player.favorites.push(id);
    first.ui.player.volume = 1.75;
    first.ui.player.mini_width = Some(620.0);
    player::save_library(&mut first, path.to_str().unwrap()).unwrap();
    let mut second = FilmcraftApp::new(Session::default());
    player::load_library(&mut second, path.to_str().unwrap()).unwrap();
    assert_eq!(second.ui.player.dislikes, vec![id]);
    assert_eq!(second.ui.player.favorites, vec![id]);
    assert_eq!(second.ui.player.volume, 1.75);
    assert_eq!(second.ui.player.mini_width, Some(620.0));
    assert_eq!(second.session.project.items.len(), first.session.project.items.len());
}

#[test]
fn closing_persists_preferences_without_opening_a_save_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.fcproj").to_string_lossy().into_owned();
    let mut original = app();
    original.player.library_path = Some(path.clone());
    original.ui.player.volume = 0.37;
    player::persist(&mut original);
    assert!(original.dialog.is_none());
    let mut restored = FilmcraftApp::new(Session::default());
    player::load_library(&mut restored, &path).unwrap();
    assert_eq!(original.session.project.items.len(), restored.session.project.items.len());
    assert_eq!(restored.ui.player.volume, 0.37);
}

#[test]
fn source_gpu_plan_preserves_reference_pixels_and_handles_missing_media() {
    let app = app();
    let project = app.session.project.clone();
    let provider = app.session.media.provider(project.clone(), app.session.services.clone());
    let id = *project.items.keys().next().unwrap();
    let plan = filmcraft_render::plan::plan_item(&project, id, Tick::ZERO, 1.0, &provider);
    let expected = filmcraft_render::render_item(&project, id, Tick::ZERO, 1.0, &provider).unwrap();
    let actual = filmcraft_render::plan::execute_cpu(&plan);
    assert_eq!((actual.w, actual.h), (expected.w, expected.h));
    for (a, b) in actual.px.iter().zip(&expected.px) {
        assert!((a - b).abs() < 0.001);
    }
    let missing = filmcraft_render::plan::plan_item(&project, filmcraft_project::ItemId(u64::MAX), Tick::ZERO, 1.0, &provider);
    let missing = filmcraft_render::plan::execute_cpu(&missing);
    assert_eq!((missing.w, missing.h), (1, 1));
}

#[test]
fn window_close_does_not_prompt_to_save_imported_videos() {
    let mut h = harness(1200.0);
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
    h.step();
    assert!(h.state().dialog.is_none(), "player close must not show an editor Save Project dialog");
    assert!(!h.output().viewport_output[&egui::ViewportId::ROOT].commands.contains(&egui::ViewportCommand::CancelClose));
}

#[test]
fn large_library_only_builds_visible_grid_rows() {
    let mut app = app();
    for _ in 0..12 {
        app.session.execute("file.importDemoFootage", json!({})).unwrap();
    }
    assert!(app.session.project.items.len() > 60);
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_| app);
    h.step();
    let visible = h.state().auto.query("player.video.").len();
    assert!(visible > 0 && visible < 20, "only visible rows should request thumbnails: {visible}");
}

#[test]
fn library_grid_fills_tall_window() {
    let mut app = app();
    for _ in 0..12 {
        app.session.execute("file.importDemoFootage", json!({})).unwrap();
    }
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1200.0)).build_eframe(move |_| app);
    for _ in 0..3 {
        h.step();
    }
    let grid = h.state().auto.find("player.grid.viewport").unwrap();
    assert!(grid.rect[3] > 900.0, "grid should use available height: {:?}", grid.rect);
    assert!((grid.rect[1] + grid.rect[3] - 1200.0).abs() < 1.0, "grid should meet window edge without a bottom margin");
}

#[test]
fn player_replaces_queue_with_related_videos_and_miniplayer_keeps_playing() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().auto.query("player.queue.").is_empty());
    assert!(h.state().auto.find("player.next").is_none());
    assert!(h.state().auto.find("player.nav.watch").is_none());
    assert!(!h.state().auto.query("player.related.").is_empty());
    assert!(h.state().auto.find("player.like").is_some());
    player::minimize(h.state_mut());
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().ui.player.miniplayer);
    assert!(h.state().source_playback.clock.playing);
    assert!(h.state().auto.find("player.mini.picture").is_some());
    assert!(h.state().auto.find("player.mini.seek").is_some());
    assert!(!h.state().auto.query("player.video.").is_empty());
}

#[test]
fn viewing_hotkeys_switch_mode_and_adjust_volume() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().ui.player.volume = 0.5;
    let key = |h: &mut Harness<'_, FilmcraftApp>, key| {
        for pressed in [true, false] {
            h.event(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
            h.step();
        }
    };
    key(&mut h, egui::Key::ArrowUp);
    assert!((h.state().ui.player.volume - 0.55).abs() < 0.001);
    h.state_mut().ui.player.volume = 1.98;
    key(&mut h, egui::Key::ArrowUp);
    assert_eq!(h.state().ui.player.volume, 2.0);
    key(&mut h, egui::Key::ArrowUp);
    assert_eq!(h.state().ui.player.volume, 2.0);
    key(&mut h, egui::Key::ArrowDown);
    assert!((h.state().ui.player.volume - 1.95).abs() < 0.001);
    key(&mut h, egui::Key::T);
    assert!(h.state().ui.player.theater);
    key(&mut h, egui::Key::I);
    assert!(h.state().ui.player.miniplayer);
    key(&mut h, egui::Key::I);
    assert!(!h.state().ui.player.miniplayer);
    assert_eq!(h.state().ui.player.page, "watch");
}

#[test]
fn video_boxes_and_grid_crops_preserve_display_aspect() {
    for aspect in [4.0 / 3.0, 16.0 / 9.0, 2.39, 9.0 / 16.0] {
        let size = player::fitted_video_size(1000.0, 500.0, aspect);
        assert!(size.x <= 1000.0 && size.y <= 500.01);
        assert!((size.x / size.y - aspect).abs() < 0.001);
        let uv = player::thumbnail_uv(aspect, 16.0 / 9.0);
        assert!((aspect * uv.width() / uv.height() - 16.0 / 9.0).abs() < 0.001);
        assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0);
    }
}
#[test]
fn miniplayer_preserves_theater_preference_and_theater_has_a_grid() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().ui.player.theater = true;
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().auto.find("player.theater.grid").is_some(), "theater has a video grid below the movie");
    let picture = h.state().auto.find("player.picture").unwrap();
    for card in h.state().auto.query("player.video.") {
        assert!(card.rect[1] > picture.rect[1] + picture.rect[3]);
    }
    player::minimize(h.state_mut());
    assert!(h.state().ui.player.theater);
    assert!(h.state().ui.player.miniplayer);
}

#[test]
fn player_cursors_distinguish_clicks_text_and_dragging() {
    fn hover(h: &mut Harness<'_, FilmcraftApp>, id: &str, fx: f32, fy: f32) -> egui::Pos2 {
        let r = h.state().auto.find(id).unwrap().rect;
        let p = egui::pos2(r[0] + r[2] * fx, r[1] + r[3] * fy);
        h.event(egui::Event::PointerMoved(p));
        for _ in 0..3 {
            h.step();
        }
        p
    }
    let mut h = harness(1600.0);
    hover(&mut h, "player.search", 0.5, 0.5);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Text);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().stop_source();
    for _ in 0..3 {
        h.step();
    }
    hover(&mut h, "player.play", 0.5, 0.5);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
    let p = hover(&mut h, "player.seek", 0.3, 0.5);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    let p = p + egui::vec2(30.0, 0.0);
    h.event(egui::Event::PointerMoved(p));
    h.step();
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    player::minimize(h.state_mut());
    for _ in 0..4 {
        h.step();
    }
    hover(&mut h, "player.mini.picture", 0.25, 0.4);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grab);
    for control in ["player.play", "player.mini.close", "player.mini.restore"] {
        hover(&mut h, control, 0.5, 0.5);
        assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "{control}");
    }
    let p = hover(&mut h, "player.mini.picture", 0.25, 0.4);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerMoved(p - egui::vec2(30.0, 0.0)));
    h.step();
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grabbing);
}

#[test]
fn miniplayer_resize_preserves_aspect_anchor_and_playback() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    player::minimize(h.state_mut());
    for _ in 0..4 {
        h.step();
    }
    let before = h.state().auto.find("player.mini.picture").unwrap().rect;
    let card = h.state().auto.find("player.mini.card").unwrap().rect;
    let grip = h.state().auto.find("player.mini.resize").unwrap().rect;
    let start = egui::pos2(grip[0] + grip[2] * 0.5, grip[1] + grip[3] * 0.5);
    h.event(egui::Event::PointerMoved(start));
    h.step();
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::ResizeNwSe);
    h.event(egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    let end = start - egui::vec2(120.0, 70.0);
    h.event(egui::Event::PointerMoved(end));
    h.step();
    h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    for _ in 0..4 {
        h.step();
    }
    let after = h.state().auto.find("player.mini.picture").unwrap().rect;
    let resized = h.state().auto.find("player.mini.card").unwrap().rect;
    assert!(after[2] > before[2] + 80.0, "{before:?} -> {after:?}");
    assert!((before[2] / before[3] - after[2] / after[3]).abs() < 0.01);
    assert!((card[0] + card[2] - resized[0] - resized[2]).abs() < 1.0);
    assert!((card[1] + card[3] - resized[1] - resized[3]).abs() < 1.0);
    assert!(h.state().source_playback.clock.playing);
    assert!(h.state().ui.player.miniplayer);
    assert!(h.state().ui_error.is_none());
    if let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(dir.join("miniplayer-resized.png")).unwrap();
    }
    for width in [f32::NAN, f32::INFINITY, -100.0, f32::MAX] {
        h.state_mut().ui.player.mini_width = Some(width);
        for _ in 0..4 {
            h.step();
        }
        let rect = h.state().auto.find("player.mini.card").unwrap().rect;
        assert!(rect.iter().all(|n| n.is_finite()));
        assert!(rect[0] >= 0.0 && rect[1] >= 0.0);
        assert!(rect[0] + rect[2] <= 1600.5 && rect[1] + rect[3] <= 900.5, "{rect:?}");
        assert!(h.state().ui_error.is_none());
    }
}

#[test]
fn miniplayer_inner_edges_show_double_arrows_and_resize() {
    for (target, cursor, delta) in [
        ("player.mini.resize.horizontal", egui::CursorIcon::ResizeHorizontal, egui::vec2(-80.0, 0.0)),
        ("player.mini.resize.vertical", egui::CursorIcon::ResizeVertical, egui::vec2(0.0, -60.0)),
    ] {
        let mut h = harness(1600.0);
        let id = *h.state().session.project.items.keys().next().unwrap();
        player::open(h.state_mut(), id);
        player::minimize(h.state_mut());
        for _ in 0..4 {
            h.step();
        }
        let before = h.state().auto.find("player.mini.picture").unwrap().rect;
        let r = h.state().auto.find(target).unwrap().rect;
        let start = egui::pos2(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        h.event(egui::Event::PointerMoved(start));
        for _ in 0..3 {
            h.step();
        }
        assert_eq!(h.output().platform_output.cursor_icon, cursor, "{target}");
        h.event(egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.step();
        let end = start + delta;
        h.event(egui::Event::PointerMoved(end));
        h.step();
        assert_eq!(h.output().platform_output.cursor_icon, cursor, "{target}");
        h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        for _ in 0..4 {
            h.step();
        }
        let after = h.state().auto.find("player.mini.picture").unwrap().rect;
        assert!(after[2] > before[2] + 60.0, "{target}: {before:?} -> {after:?}");
        assert!((before[2] / before[3] - after[2] / after[3]).abs() < 0.01);
        assert!(h.state().source_playback.clock.playing);
        assert!(h.state().ui.player.miniplayer);
        assert!(h.state().ui_error.is_none());
    }
}

#[test]
fn missing_videos_are_removed_but_existing_unreadable_files_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("unreadable.mp4");
    std::fs::write(&existing, b"not a video").unwrap();
    let missing = dir.path().join("deleted.mp4");
    let mut app = app();
    let ids: Vec<_> = app.session.project.items.keys().copied().collect();
    for (id, path) in [(ids[0], &missing), (ids[1], &existing)] {
        std::sync::Arc::make_mut(&mut app.session.project).item_mut(id).unwrap().as_media_mut().unwrap().media =
            filmcraft_project::MediaRef::File { path: path.to_string_lossy().into_owned() };
    }
    app.ui.player.favorites = vec![ids[0], ids[1]];
    app.ui.player.history = vec![ids[0]];
    app.ui.player.dislikes = vec![ids[0]];
    app.session.state.project_selection = vec![ids[1]];
    app.session.offline.prompt = true;
    app.ui.status.clear();
    assert_eq!(player::prune_missing_videos(&mut app).unwrap(), 1);
    assert!(app.ui.status.is_empty(), "cleanup must not display a notification");
    assert!(app.session.project.item(ids[0]).is_none());
    assert!(app.session.project.item(ids[1]).is_some());
    assert_eq!(app.session.project.items.len(), ids.len() - 1);
    assert_eq!(app.ui.player.favorites, vec![ids[1]]);
    assert!(app.ui.player.history.is_empty());
    assert!(app.ui.player.dislikes.is_empty());
    assert_eq!(app.session.state.project_selection, vec![ids[1]]);
    assert!(!app.session.offline.prompt);
    assert!(app.ui.link_media.is_none());
    assert!(existing.exists());
    assert_eq!(player::prune_missing_videos(&mut app).unwrap(), 0);
    let library = dir.path().join("library.fcproj");
    player::save_library(&mut app, library.to_str().unwrap()).unwrap();
    let mut restored = self::app();
    player::load_library(&mut restored, library.to_str().unwrap()).unwrap();
    assert!(restored.session.project.item(ids[0]).is_none());
    assert!(restored.session.project.item(ids[1]).is_some());
}

#[test]
fn loading_library_prunes_deleted_files_and_never_opens_link_media() {
    let dir = tempfile::tempdir().unwrap();
    let mut original = app();
    let id = *original.session.project.items.keys().next().unwrap();
    std::sync::Arc::make_mut(&mut original.session.project).item_mut(id).unwrap().as_media_mut().unwrap().media =
        filmcraft_project::MediaRef::File { path: dir.path().join("moved-away.mp4").to_string_lossy().into_owned() };
    original.ui.player.favorites = vec![id];
    let library = dir.path().join("library.fcproj");
    player::save_library(&mut original, library.to_str().unwrap()).unwrap();
    let mut h = harness(1600.0);
    player::load_library(h.state_mut(), library.to_str().unwrap()).unwrap();
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().session.project.item(id).is_none());
    assert!(h.state().ui.player.favorites.is_empty());
    assert!(h.state().ui.link_media.is_none());
    assert!(!h.state().session.offline.prompt);
    assert!(h.state().auto.query("linkMedia.").is_empty());
    assert!(h.state().ui_error.is_none());
}

#[test]
fn player_menus_use_fork_identity_and_updates_stay_local() {
    let mut app = app();
    let items = filmcraft_ui_egui::menus::menu_items(&app);
    for (id, expected) in [("app.about", "About YouCraft"), ("help.github", "Upstream source code")] {
        assert_eq!(items.iter().find(|item| item.id == id).unwrap().label, expected);
    }
    let before = serde_json::to_value(&app.session.project).unwrap();
    filmcraft_ui_egui::menus::invoke(&mut app, &egui::Context::default(), "app.checkForUpdates", json!({})).unwrap();
    assert!(app.ui.status.contains("YouCraft is a source build"));
    assert!(app.dialog.is_none());
    assert_eq!(serde_json::to_value(&app.session.project).unwrap(), before);
}

#[test]
fn theater_scroll_over_grid_moves_picture_and_grid_together() {
    let mut h = harness(1600.0);
    for _ in 0..8 {
        h.state_mut().session.execute("file.importDemoFootage", json!({})).unwrap();
    }
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().stop_source();
    h.state_mut().ui.player.theater = true;
    h.run_steps(4);
    fn wheel(h: &mut Harness<'_, FilmcraftApp>, position: egui::Pos2, delta: f32) {
        h.event(egui::Event::PointerMoved(position));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            source: egui::MouseWheelSource::Wheel,
            delta: egui::vec2(0.0, delta),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(20);
    }
    wheel(&mut h, egui::pos2(800.0, 400.0), -650.0);
    let picture = h.state().auto.find("player.picture").unwrap().rect;
    let heading = h.state().auto.find("player.theater.grid").unwrap().rect;
    let grid = h.state().auto.find("player.grid.viewport").unwrap().rect;
    let position = egui::pos2(grid[0] + grid[2] * 0.5, (grid[1] + 80.0).min(850.0));
    wheel(&mut h, position, -180.0);
    let moved_picture = h.state().auto.find("player.picture").unwrap().rect;
    let moved_heading = h.state().auto.find("player.theater.grid").unwrap().rect;
    assert!(moved_picture[1] < picture[1] - 100.0, "scrolling over the grid must move the video");
    assert!(((moved_picture[1] - picture[1]) - (moved_heading[1] - heading[1])).abs() < 1.0);
    assert!(h.state().auto.query("player.video.").len() < 20, "offscreen grid rows stay virtualized");
    if let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        h.render().unwrap().save(dir.join("theater-scrolled.png")).unwrap();
    }
}

#[test]
fn fullscreen_more_videos_browses_without_pausing_and_selects_in_place() {
    for width in [640.0, 1600.0] {
        let mut h = harness(width);
        let id = *h.state().session.project.items.keys().next().unwrap();
        player::open(h.state_mut(), id);
        h.state_mut().source_playback.clock.looping = true;
        h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().fullscreen = Some(true);
        h.event(egui::Event::PointerMoved(egui::pos2(200.0, 200.0)));
        h.run_steps(4);
        fn key(h: &mut Harness<'_, FilmcraftApp>, key: egui::Key) {
            for pressed in [true, false] {
                h.event(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
                h.step();
            }
            h.run_steps(3);
        }
        fn click(h: &mut Harness<'_, FilmcraftApp>, id: &str) {
            let r = h.state().auto.find(id).unwrap().rect;
            let pos = egui::pos2(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
            h.event(egui::Event::PointerMoved(pos));
            h.step();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
                h.step();
            }
            h.run_steps(3);
        }
        let more = h.state().auto.find("player.fullscreen.more").unwrap().rect;
        let seek = h.state().auto.find("player.seek").unwrap().rect;
        let picture = h.state().auto.find("player.picture").unwrap().rect;
        assert!(more[1] >= seek[1] + seek[3], "More videos sits below the progress bar");
        assert!(((more[0] + more[2] * 0.5) - (seek[0] + seek[2] * 0.5)).abs() < 1.0);
        for control in ["player.play", "player.mute", "player.fullscreen", "player.audio", "player.captions"] {
            let r = h.state().auto.find(control).unwrap().rect;
            assert!(
                more[0] + more[2] <= r[0] || r[0] + r[2] <= more[0] || more[1] + more[3] <= r[1] || r[1] + r[3] <= more[1],
                "More videos overlaps {control}"
            );
        }
        if let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR") {
            let dir = std::path::Path::new(&dir);
            std::fs::create_dir_all(dir).unwrap();
            h.render().unwrap().save(dir.join(format!("fullscreen-more-button-{width}.png"))).unwrap();
        }
        h.event(egui::Event::PointerMoved(egui::pos2(width * 0.5, 300.0)));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            source: egui::MouseWheelSource::Wheel,
            delta: egui::vec2(0.0, -120.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(6);
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_some(), "scrolling down reveals more videos");
        assert!(h.state().source_playback.clock.playing);
        let moved_seek = h.state().auto.find("player.seek").unwrap().rect;
        let moved_picture = h.state().auto.find("player.picture").unwrap().rect;
        let movement = seek[1] - moved_seek[1];
        assert!(movement > 5.0 && movement <= 125.0, "wheel movement is gradual: {movement}");
        assert!((picture[1] - moved_picture[1]).abs() < 1.0, "the video stays fixed while transport moves");
        if width == 1600.0
            && let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR")
        {
            h.render().unwrap().save(std::path::Path::new(&dir).join("fullscreen-partial-scroll.png")).unwrap();
        }
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            source: egui::MouseWheelSource::Wheel,
            delta: egui::vec2(0.0, 120.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(6);
        assert!((h.state().auto.find("player.seek").unwrap().rect[1] - seek[1]).abs() < 2.0, "reverse scrolling restores the transport");
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_none());
        click(&mut h, "player.fullscreen.more");
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_some());
        assert!(h.state().source_playback.clock.playing, "opening the drawer does not pause playback");
        assert!(h.state().auto.find(&format!("player.video.{}", id.0)).is_none());
        let cards = h.state().auto.query("player.video.");
        assert!(!cards.is_empty());
        for card in &cards {
            assert!(card.rect[0] >= 0.0 && card.rect[0] + card.rect[2] <= width);
        }
        let selected = cards[0].id.clone();
        let selected_id = selected.strip_prefix("player.video.").unwrap().parse::<u64>().unwrap();
        if width == 1600.0
            && let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR")
        {
            let dir = std::path::Path::new(&dir);
            std::fs::create_dir_all(dir).unwrap();
            h.render().unwrap().save(dir.join("fullscreen-more-videos.png")).unwrap();
        }
        click(&mut h, "player.fullscreen.more.close");
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_none());
        key(&mut h, egui::Key::V);
        key(&mut h, egui::Key::Escape);
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_none());
        assert!(h.input_mut().viewports[&egui::ViewportId::ROOT].fullscreen.unwrap());
        key(&mut h, egui::Key::V);
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_some());
        click(&mut h, &selected);
        assert_eq!(h.state().session.state.source_item.unwrap().0, selected_id);
        assert!(h.state().source_playback.clock.playing);
        assert!(h.state().auto.find("player.fullscreen.more.grid").is_none());
        assert_eq!(h.state().ui.player.page, "watch");
        assert!(h.input_mut().viewports[&egui::ViewportId::ROOT].fullscreen.unwrap());
        h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().fullscreen = Some(false);
        h.run_steps(3);
        key(&mut h, egui::Key::V);
        assert!(h.state().auto.find("player.fullscreen.more").is_none());
        assert!(h.state().ui_error.is_none());
    }
}

#[test]
fn scrolled_fullscreen_transport_does_not_seek_or_toggle_playback() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().stop_source();
    player::seek(h.state_mut(), Tick::from_seconds_f64(5.0));
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().fullscreen = Some(true);
    h.run_steps(4);
    h.event(egui::Event::PointerMoved(egui::pos2(800.0, 400.0)));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        source: egui::MouseWheelSource::Wheel,
        delta: egui::vec2(0.0, -240.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(6);
    let before = h.state().session.state.source_playhead;
    let seek = h.state().auto.find("player.seek").unwrap().rect;
    let start = egui::pos2(seek[0] + seek[2] * 0.8, seek[1] + seek[3] * 0.5);
    h.event(egui::Event::PointerMoved(start));
    h.step();
    assert!(h.state().auto.find("player.seek.preview").is_none(), "faded rail has no hover preview");
    h.event(egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    let end = start + egui::vec2(80.0, 0.0);
    h.event(egui::Event::PointerMoved(end));
    h.step();
    h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(6);
    assert_eq!(h.state().session.state.source_playhead, before);
    assert!(!h.state().source_playback.clock.playing);
    if let Some(dir) = std::env::var_os("FILMCRAFT_UI_SNAPSHOT_DIR") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        h.render().unwrap().save(dir.join("fullscreen-disabled-rail.png")).unwrap();
    }
    // Returning to the top restores seeking.
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        source: egui::MouseWheelSource::Wheel,
        delta: egui::vec2(0.0, 500.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(6);
    let seek = h.state().auto.find("player.seek").unwrap().rect;
    let pos = egui::pos2(seek[0] + seek[2] * 0.8, seek[1] + seek[3] * 0.5);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    assert!(h.state().session.state.source_playhead > before);
}

#[test]
fn tiny_fullscreen_scroll_settles_back_but_browsing_scroll_stays_open() {
    let mut h = harness(1600.0);
    let id = *h.state().session.project.items.keys().next().unwrap();
    player::open(h.state_mut(), id);
    h.state_mut().stop_source();
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().fullscreen = Some(true);
    h.run_steps(4);
    let initial = h.state().auto.find("player.seek").unwrap().rect;
    for (delta, should_return) in [(-24.0, true), (-240.0, false)] {
        h.event(egui::Event::PointerMoved(egui::pos2(800.0, 400.0)));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            source: egui::MouseWheelSource::Wheel,
            delta: egui::vec2(0.0, delta),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(30);
        let after = h.state().auto.find("player.seek").unwrap().rect;
        if should_return {
            assert!((initial[1] - after[1]).abs() < 1.0, "small scroll returns to full video");
        } else {
            assert!(after[1] < initial[1] - 150.0, "larger scroll stays open");
        }
    }
}
