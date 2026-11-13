//! Library persistence, missing-file cleanup, and discovery results.

use super::{FilmcraftApp, PlayerState};
use serde_json::json;
use std::sync::atomic::Ordering;

/// Remove stale library references only when the host confirms the file no longer exists.
/// Permission, device and decoder failures must not discard otherwise valid library entries.
pub fn prune_missing_videos(app: &mut FilmcraftApp) -> Result<usize, String> {
    let missing: Vec<_> = app
        .session
        .project
        .items
        .values()
        .filter_map(|item| {
            let filmcraft_project::MediaRef::File { path } = &item.as_media()?.media else { return None };
            app.session.services.file_size(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound).then_some(item.id)
        })
        .collect();
    app.session.offline.prompt = false;
    app.ui.link_media = None;
    if missing.is_empty() {
        return Ok(0);
    }
    if app.session.state.source_item.is_some_and(|id| missing.contains(&id)) {
        app.stop_source();
        app.ui.player.miniplayer = false;
        app.ui.player.page = "library".into();
    }
    let selection = app.session.state.project_selection.clone();
    app.session.execute("project.select", json!({"items": missing})).map_err(|error| error.to_string())?;
    let result = app.session.execute("project.delete", json!({"items": missing}));
    app.session.state.project_selection = selection.into_iter().filter(|id| app.session.project.item(*id).is_some()).collect();
    app.session.offline.prompt = false;
    result.map_err(|error| error.to_string())?;
    let project = &app.session.project;
    app.ui.player.history.retain(|id| project.item(*id).is_some());
    app.ui.player.favorites.retain(|id| project.item(*id).is_some());
    app.ui.player.dislikes.retain(|id| project.item(*id).is_some());
    app.player.thumbnails = None;
    app.player.thumbnail_failures.clear();
    let removed = missing.iter().filter(|id| project.item(**id).is_none()).count();
    Ok(removed)
}

/// Library media uses the upstream atomic project format; viewing preferences use a companion file.
pub fn save_library(app: &mut FilmcraftApp, path: &str) -> Result<(), String> {
    app.session.execute("file.saveAs", json!({"path":path})).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&app.ui.player).map_err(|e| e.to_string())?;
    app.session
        .services
        .write_file(&format!("{path}.youcraft.json"), &bytes)
        .map_err(|e| format!("Library saved, but viewing preferences could not be saved: {e}"))?;
    app.ui.status = "Library and viewing preferences saved".into();
    Ok(())
}
pub fn load_library(app: &mut FilmcraftApp, path: &str) -> Result<(), String> {
    app.stop_source();
    app.session.execute("file.open", json!({"path":path})).map_err(|e| e.to_string())?;
    prune_missing_videos(app)?;
    app.ui.player = PlayerState::default();
    app.player.thumbnails = None;
    app.player.thumbnail_failures.clear();
    let companion = format!("{path}.youcraft.json");
    if app.session.services.file_size(&companion).is_ok_and(|size| size <= 1024 * 1024) {
        let data = app.session.services.read_range(&companion, 0, 1024 * 1024).map_err(|e| e.to_string())?;
        let mut state: PlayerState = serde_json::from_slice(&data).map_err(|e| format!("Opened library; viewing preferences are invalid: {e}"))?;
        if !state.volume.is_finite() {
            state.volume = 1.0;
        }
        state.volume = state.volume.clamp(0.0, 2.0);
        state.dislikes.retain(|id| app.session.project.item(*id).is_some());
        state.miniplayer = false;
        state.audio_stream = 0;
        state.history.retain(|id| app.session.project.item(*id).is_some());
        state.favorites.retain(|id| app.session.project.item(*id).is_some());
        state.page = "library".into();
        state.theater = false;
        app.ui.player = state;
    }
    Ok(())
}
pub(super) fn dialog(app: &mut FilmcraftApp, save: bool) {
    use crate::native_dialogs::FileDialog;
    let dialog = if save { FileDialog::SaveProject { name: "YouCraft library.fcproj".into() } } else { FileDialog::OpenProject };
    let result = app.pick(dialog, move |app, paths| {
        if let Some(path) = paths.first() {
            if save {
                save_library(app, path)?;
            } else {
                load_library(app, path)?;
            }
        }
        Ok(serde_json::Value::Null)
    });
    if let Err(e) = result {
        app.ui.status = e;
    }
}

/// Drain one already-probed video per frame. Disk/container work runs in the native host.
pub(super) fn poll_discovery(app: &mut FilmcraftApp) {
    let playing = app.source_playback.clock.playing;
    app.player.discovery_paused.store(playing, Ordering::Relaxed);
    if playing {
        return;
    }
    let event = app.player.discovery.as_ref().map(|rx| rx.try_recv());
    match event {
        Some(Ok(Ok((path, source)))) => {
            let exists = app
                .session
                .project
                .items
                .values()
                .any(|i| i.as_media().is_some_and(|m| matches!(&m.media, filmcraft_project::MediaRef::File { path: p } if p == &path)));
            if !exists {
                app.session.media.prepare_import(path.clone(), source);
                if let Err(e) = app.session.execute("file.import", json!({"paths":[path]})) {
                    app.ui.status = format!("Could not add discovered video: {e}");
                }
            }
        }
        Some(Ok(Err(e))) => app.ui.status = e,
        Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
            app.player.discovery = None;
        }
        _ => {}
    }
}

/// Closing a player never opens the editor's Save Project dialog.
pub fn persist(app: &mut FilmcraftApp) {
    app.stop_source();
    app.player.discovery = None;
    if let Some(path) = app.player.library_path.clone()
        && let Err(e) = save_library(app, &path)
    {
        log::error!("Could not persist YouCraft library: {e}");
    }
}
