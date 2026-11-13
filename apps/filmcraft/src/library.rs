//! Background discovery of personal videos; project imports remain engine commands.
use filmcraft_ui_egui::{FilmcraftApp, player};
use std::{collections::HashSet, path::PathBuf, sync::atomic::Ordering};

fn roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) else { return vec![] };
    let mut roots: Vec<_> = ["Videos", "Movies", "Downloads", "Desktop", "Documents"].iter().map(|s| home.join(s)).collect();
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=')
                && ["XDG_VIDEOS_DIR", "XDG_DOWNLOAD_DIR", "XDG_DESKTOP_DIR", "XDG_DOCUMENTS_DIR"].contains(&key.trim())
            {
                let value = value.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                let path = PathBuf::from(value);
                if path.is_absolute() && path != home {
                    roots.push(path);
                }
            }
        }
    }
    roots
}

fn videos(roots: Vec<PathBuf>) -> Vec<String> {
    let mut pending: Vec<_> = roots.into_iter().rev().map(|p| (p, 0)).collect();
    let mut visited = HashSet::new();
    let mut files = HashSet::new();
    let mut entries = 0usize;
    while let Some((dir, depth)) = pending.pop() {
        if depth > 12 || entries > 100_000 || files.len() >= 10_000 {
            continue;
        }
        let Ok(dir) = dir.canonicalize() else { continue };
        if !visited.insert(dir.clone()) {
            continue;
        }
        let Ok(children) = std::fs::read_dir(dir) else { continue };
        for child in children.flatten() {
            entries += 1;
            if entries > 100_000 || files.len() >= 10_000 {
                break;
            }
            let name = child.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target" | "vendor") {
                continue;
            }
            let Ok(kind) = child.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = child.path();
            if kind.is_dir() {
                pending.push((path, depth + 1));
            } else if kind.is_file()
                && path.extension().is_some_and(|e| {
                    matches!(
                        e.to_string_lossy().to_ascii_lowercase().as_str(),
                        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "mpeg" | "mpg" | "mts" | "m2ts" | "ogv" | "wmv"
                    )
                })
            {
                files.insert(path.to_string_lossy().into_owned());
            }
        }
    }
    let mut files: Vec<_> = files.into_iter().collect();
    files.sort();
    files
}

pub fn start(app: &mut FilmcraftApp, ctx: &egui::Context, dir: Option<PathBuf>, restore: bool, discover: bool) {
    if let Some(dir) = dir {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            app.ui.status = format!("Cannot save library: {e}");
        } else {
            let path = dir.join("library.fcproj").to_string_lossy().into_owned();
            if restore
                && std::path::Path::new(&path).is_file()
                && let Err(e) = player::load_library(app, &path)
            {
                app.ui.status = e;
            }
            let cache = dir.join("previews-v2");
            match std::fs::create_dir_all(&cache) {
                Ok(()) => app.player.preview_cache_dir = Some(cache.to_string_lossy().into_owned()),
                Err(e) => log::warn!("Preview cache unavailable: {e}"),
            }
            app.player.library_path = Some(path);
        }
    }
    // Explicit demo/empty launches stay isolated for reproducible tests and previews.
    if !discover {
        return;
    }
    let existing: HashSet<_> = app
        .session
        .project
        .items
        .values()
        .filter_map(|i| match &i.as_media()?.media {
            filmcraft_project::MediaRef::File { path } => Some(path.clone()),
            _ => None,
        })
        .collect();
    let pool = app.session.media.clone();
    let services = app.session.services.clone();
    let paused = app.player.discovery_paused.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    app.player.discovery = Some(rx);
    let ctx = ctx.clone();
    let result = std::thread::Builder::new().name("youcraft-discovery".into()).spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for path in videos(roots()) {
                if existing.contains(&path) {
                    continue;
                }
                while paused.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                match pool.probe_file(&path, &*services) {
                    Ok(source) if source.info().video.is_some() => {
                        if tx.send(Ok((path, source))).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                    Ok(_) => {}
                    Err(e) => log::debug!("Skipped video {path}: {e}"),
                }
            }
        }));
        if outcome.is_err() {
            let _ = tx.send(Err("Video discovery stopped unexpectedly. You can still open videos manually.".into()));
        }
        ctx.request_repaint();
    });
    if let Err(e) = result {
        app.player.discovery = None;
        app.ui.status = format!("Cannot start video discovery: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_nested_videos_once_and_skips_hidden_and_build_files() {
        let dir = std::env::temp_dir().join(format!("youcraft-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        for file in ["nested/film.MP4", "note.txt", ".hidden/private.mp4", "target/generated.mp4"] {
            std::fs::write(dir.join(file), []).unwrap();
        }
        let found = videos(vec![dir.clone(), dir.join("nested")]);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("film.MP4"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
