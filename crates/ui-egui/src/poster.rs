//! Existing artwork for library cards, independent of the video decoder.
//! Desktop cache: Freedesktop Thumbnail Managing Standard 0.9.0 (URI/mtime/size validation).

use crate::frames::Rgba;
use filmcraft_engine::Services;
use filmcraft_project::{ItemId, MediaRef, Project};

const MAX_IMAGE: usize = 16 << 20;

pub fn load(project: &Project, item: ItemId, width: u32, services: &dyn Services) -> Option<Rgba> {
    let MediaRef::File { path } = &project.item(item)?.as_media()?.media else { return None };
    #[cfg(target_os = "linux")]
    if let Some(image) = desktop(path, width) {
        return Some(image);
    }
    let reader = services.reader(path)?.ok()?;
    let bytes = filmcraft_media::artwork::embedded(reader.as_ref()).ok()??;
    decode(&bytes, width)
}

/// Generated frames carry source pixels; artwork already carries display pixels.
/// Normalize only generated card images so anamorphic videos keep their display aspect.
pub fn display_pixels(image: Rgba, par: (u32, u32), width: u32) -> Rgba {
    let (n, d) = filmcraft_project::sane_par(par);
    if n == d || image.w <= 1 || image.h <= 1 {
        return image;
    }
    let display_width = image.w as f64 * f64::from(n) / f64::from(d);
    let scale = (f64::from(width.clamp(1, 1024)) / display_width).min(1024.0 / image.h as f64).min(1.0);
    let w = (display_width * scale).round().clamp(1.0, 1024.0) as u32;
    let h = (image.h as f64 * scale).round().clamp(1.0, 1024.0) as u32;
    let (Ok(src_w), Ok(src_h)) = (u32::try_from(image.w), u32::try_from(image.h)) else { return image };
    let Some(source) = image::RgbaImage::from_raw(src_w, src_h, image.px) else {
        return Rgba { w: 1, h: 1, px: vec![0, 0, 0, 255] };
    };
    let resized = image::imageops::resize(&source, w, h, image::imageops::FilterType::Triangle);
    Rgba { w: w as usize, h: h as usize, px: resized.into_raw() }
}

fn decode(bytes: &[u8], width: u32) -> Option<Rgba> {
    if bytes.len() > MAX_IMAGE {
        return None;
    }
    let format = image::guess_format(bytes).ok()?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let image = decoded.thumbnail(width.clamp(1, 1024).min(decoded.width()), 1024.min(decoded.height())).to_rgba8();
    Some(Rgba { w: image.width() as usize, h: image.height() as usize, px: image.into_raw() })
}

#[cfg(target_os = "linux")]
fn desktop(path: &str, width: u32) -> Option<Rgba> {
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".cache")))?;
    desktop_at(std::path::Path::new(path), &root, width)
}

#[cfg(target_os = "linux")]
fn desktop_at(path: &std::path::Path, root: &std::path::Path, width: u32) -> Option<Rgba> {
    use md5::{Digest, Md5};
    use std::io::Read;
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let mtime = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    // A symlink may have a thumbnail under its own URI or the resolved target's URI.
    let absolute = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().ok()?.join(path) };
    let canonical = path.canonicalize().ok()?;
    for path in [&absolute, &canonical] {
        let uri = file_uri(path);
        let name = format!("{:x}.png", Md5::digest(uri.as_bytes()));
        for size in ["xx-large", "x-large", "large", "normal"] {
            let candidate = root.join("thumbnails").join(size).join(&name);
            let Ok(file) = std::fs::File::open(candidate) else { continue };
            if file.metadata().ok().is_none_or(|m| m.len() > MAX_IMAGE as u64) {
                continue;
            }
            let mut bytes = Vec::new();
            if file.take((MAX_IMAGE + 1) as u64).read_to_end(&mut bytes).is_err() {
                continue;
            }
            if valid_metadata(&bytes, &uri, mtime, metadata.len())
                && let Some(image) = decode(&bytes, width)
            {
                return Some(image);
            }
        }
        if absolute == canonical {
            break;
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn file_uri(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(char::from(b));
        } else {
            use std::fmt::Write;
            let _ = write!(uri, "%{b:02X}");
        }
    }
    uri
}

/// Only the uncompressed tEXt fields specified by the desktop standard establish freshness.
#[cfg(target_os = "linux")]
fn valid_metadata(bytes: &[u8], uri: &str, mtime: u64, size: u64) -> bool {
    fn fields(bytes: &[u8], uri: &str, mtime: u64, size: u64) -> Option<bool> {
        if bytes.len() > MAX_IMAGE || bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" {
            return None;
        }
        let (mut found_uri, mut found_time) = (false, false);
        let mut offset = 8usize;
        for _ in 0..4096 {
            let len = u32::from_be_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?) as usize;
            let kind = bytes.get(offset.checked_add(4)?..offset.checked_add(8)?)?;
            let start = offset.checked_add(8)?;
            let end = start.checked_add(len)?;
            let data = bytes.get(start..end)?;
            offset = end.checked_add(4)?;
            bytes.get(end..offset)?;
            if kind == b"tEXt" {
                let split = data.iter().position(|b| *b == 0)?;
                let key = data.get(..split)?;
                if !matches!(key, b"Thumb::URI" | b"Thumb::MTime" | b"Thumb::Size") {
                    continue;
                }
                let value = std::str::from_utf8(data.get(split.checked_add(1)?..)?).ok()?;
                match key {
                    b"Thumb::URI" => {
                        if value != uri {
                            return Some(false);
                        }
                        found_uri = true;
                    }
                    b"Thumb::MTime" => {
                        if value.parse::<u64>().ok()? != mtime {
                            return Some(false);
                        }
                        found_time = true;
                    }
                    b"Thumb::Size" if value.parse::<u64>().ok()? != size => return Some(false),
                    _ => {}
                }
            }
            if kind == b"IEND" {
                return Some(found_uri && found_time);
            }
        }
        None
    }
    fields(bytes, uri, mtime, size).unwrap_or(false)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use md5::{Digest, Md5};
    fn png(uri: &str, time: u64, size: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgb);
            encoder.add_text_chunk("Thumb::URI".into(), uri.into()).unwrap();
            encoder.add_text_chunk("Thumb::MTime".into(), time.to_string()).unwrap();
            encoder.add_text_chunk("Thumb::Size".into(), size.to_string()).unwrap();
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0]).unwrap();
        }
        bytes
    }
    #[test]
    fn desktop_cache_requires_matching_uri_time_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("film #é.mp4");
        std::fs::write(&video, b"unsupported video codec").unwrap();
        let uri = file_uri(&video);
        assert!(uri.ends_with("film%20%23%C3%A9.mp4"));
        let metadata = video.metadata().unwrap();
        let time = metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let bytes = png(&uri, time, metadata.len());
        let cache = dir.path().join("thumbnails/large");
        std::fs::create_dir_all(&cache).unwrap();
        let cached = cache.join(format!("{:x}.png", Md5::digest(uri.as_bytes())));
        std::fs::write(&cached, &bytes).unwrap();
        let image = desktop_at(&video, dir.path(), 320).unwrap();
        assert_eq!((image.w, image.h), (2, 2));
        assert!(!valid_metadata(&bytes, "file:///different.mp4", time, metadata.len()));
        assert!(!valid_metadata(&bytes, &uri, time + 1, metadata.len()));
        assert!(!valid_metadata(&bytes, &uri, time, metadata.len() + 1));
        for end in 0..bytes.len() {
            assert!(std::panic::catch_unwind(|| valid_metadata(&bytes[..end], &uri, time, metadata.len())).is_ok());
        }
        std::fs::write(&video, b"replacement").unwrap();
        assert!(desktop_at(&video, dir.path(), 320).is_none());
    }
    #[test]
    fn rejects_broken_and_excessive_images() {
        assert!(decode(b"bad jpeg", 320).is_none());
        assert!(decode(&vec![0; MAX_IMAGE + 1], 320).is_none());
    }
}
