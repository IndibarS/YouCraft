//! Bounded, disposable preview slots. Disk work runs only on the thumbnail worker.
use crate::frames::Rgba;
use filmcraft_engine::Services;
use std::hash::{Hash, Hasher};

pub fn identity(project: &filmcraft_project::Project, item: filmcraft_project::ItemId, frame: i64, width: u32, services: &dyn Services) -> Option<u64> {
    let media = project.item(item)?.as_media()?;
    let filmcraft_project::MediaRef::File { path } = &media.media else { return None };
    let size = services.file_size(path).ok()?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    "youcraft-preview-v3".hash(&mut hash);
    serde_json::to_vec(media).ok()?.hash(&mut hash);
    size.hash(&mut hash);
    services.read_range(path, 0, 4096).ok()?.hash(&mut hash);
    services.read_range(path, size.saturating_sub(4096), 4096).ok()?.hash(&mut hash);
    frame.hash(&mut hash);
    width.hash(&mut hash);
    Some(hash.finish())
}
fn slot(root: &str, key: u64) -> String {
    format!("{root}/preview-{:03}.bin", key % 512)
}
const MAX_BYTES: usize = 512 * 1024;
pub fn read(root: &str, key: u64, services: &dyn Services) -> Option<Rgba> {
    let path = slot(root, key);
    if services.file_size(&path).ok()? > MAX_BYTES as u64 {
        return None;
    }
    let bytes = services.read_range(&path, 0, MAX_BYTES).ok()?;
    decode(&bytes, key)
}
fn decode(bytes: &[u8], key: u64) -> Option<Rgba> {
    if bytes.get(..8)? != key.to_le_bytes() {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes.get(8..)?), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 << 20);
    reader.limits(limits);
    let image = reader.decode().ok()?.to_rgba8();
    Some(Rgba { w: image.width() as usize, h: image.height() as usize, px: image.into_raw() })
}
pub fn write(root: &str, key: u64, image: &Rgba, services: &dyn Services) {
    if image.w <= 1 || image.h <= 1 || image.w > 1024 || image.h > 1024 {
        return;
    }
    use image::ImageEncoder;
    let mut bytes = key.to_le_bytes().to_vec();
    if image::codecs::png::PngEncoder::new(&mut bytes).write_image(&image.px, image.w as u32, image.h as u32, image::ExtendedColorType::Rgba8).is_ok()
        && bytes.len() <= MAX_BYTES
        && let Err(error) = services.write_file(&slot(root, key), &bytes)
    {
        log::debug!("Preview cache write failed: {error}");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_rejects_collisions_and_truncated_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let image = Rgba { w: 4, h: 4, px: vec![200; 64] };
        write(root, 41, &image, &filmcraft_engine::FsServices);
        assert_eq!(read(root, 41, &filmcraft_engine::FsServices).unwrap().px, image.px);
        assert!(read(root, 553, &filmcraft_engine::FsServices).is_none());
        let bytes = std::fs::read(slot(root, 41)).unwrap();
        for length in 0..bytes.len() {
            assert!(std::panic::catch_unwind(|| decode(&bytes[..length], 41)).is_ok());
        }
    }
}
