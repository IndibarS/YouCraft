use filmcraft_ui_egui::frames;
#[allow(dead_code)]
#[path = "../src/poster.rs"]
mod poster;

#[test]
fn embedded_cover_is_available_without_a_video_decoder_and_is_not_a_seek_frame() {
    use filmcraft_engine::Session;
    use filmcraft_time::Tick;
    use filmcraft_ui_egui::{
        FilmcraftApp,
        frames::{FrameKey, Target},
    };
    use serde_json::json;
    fn atom(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut b = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend(kind);
        b.extend(data);
        b
    }
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &[255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
        2,
        2,
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
    let mut data = vec![0, 0, 0, 14, 0, 0, 0, 0];
    data.extend(png);
    let mut meta = vec![0; 4];
    meta.extend(atom(b"ilst", &atom(b"covr", &atom(b"data", &data))));
    let bytes = atom(b"moov", &atom(b"udta", &atom(b"meta", &meta)));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cover-only.mp4");
    std::fs::write(&path, bytes).unwrap();
    let mut session = Session::default();
    session.execute("file.importDemoFootage", json!({})).unwrap();
    let id = *session.project.items.keys().next().unwrap();
    std::sync::Arc::make_mut(&mut session.project).item_mut(id).unwrap().as_media_mut().unwrap().media =
        filmcraft_project::MediaRef::File { path: path.to_string_lossy().into() };
    let mut app = FilmcraftApp::new(session);
    app.player.enabled = true;
    let ctx = egui::Context::default();
    let mut result = None;
    for _ in 0..200 {
        result = app.poster(&ctx, id, Tick::ZERO, 320);
        if result.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(result.is_some(), "cover must not require playable video tracks");
    assert_eq!(result.unwrap().1, egui::vec2(2.0, 2.0));
    assert!(!app.player.thumbnail_failures.contains(&id));
    let frames = app.player.thumbnails.as_ref().unwrap();
    assert!(frames.pool.offline_status(id).is_none(), "no decoder should be opened for a poster");
    let key = FrameKey { target: Target::Item(id), frame: 0, size: 320, revision: app.session.revision, draft: false };
    assert!(frames.get(&key).is_none(), "a cover must not enter the timestamped video frame cache");
}

#[test]
fn generated_posters_preserve_non_square_pixel_aspect() {
    let image = frames::Rgba { w: 4, h: 2, px: vec![255; 4 * 2 * 4] };
    let image = poster::display_pixels(image, (2, 1), 4);
    assert_eq!((image.w, image.h), (4, 1));
    let image = poster::display_pixels(image, (0, 0), 4);
    assert_eq!((image.w, image.h), (4, 1));
}
