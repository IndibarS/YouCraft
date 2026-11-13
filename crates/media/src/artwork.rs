//! Bounded, random-access extraction of JPEG/PNG cover art; no video decoding.
//! Matroska: RFC 9559 (October 2024), Attachments and cover-art filenames.
//! MP4: QuickTime File Format (2012), metadata item/data atoms; iTunes `covr` item.

use crate::reader::ByteReader;
use std::io;

const MAX_IMAGE: u64 = 16 << 20;
const MAX_ELEMENTS: usize = 16_384;
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid or excessive cover-art metadata")
}
fn read<const N: usize>(r: &dyn ByteReader, offset: u64) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    r.read_at(offset, &mut bytes)?;
    Ok(bytes)
}
fn payload(r: &dyn ByteReader, start: u64, end: u64) -> io::Result<Vec<u8>> {
    let size = end.checked_sub(start).filter(|s| *s <= MAX_IMAGE).ok_or_else(invalid)?;
    let mut bytes = vec![0; usize::try_from(size).map_err(|_| invalid())?];
    r.read_at(start, &mut bytes)?;
    Ok(bytes)
}

pub fn embedded(r: &dyn ByteReader) -> io::Result<Option<Vec<u8>>> {
    if r.len() < 8 {
        return Ok(None);
    }
    let head = read::<8>(r, 0)?;
    let mut budget = MAX_ELEMENTS;
    if head.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        mkv(r, 0, r.len(), 0, &mut budget)
    } else if matches!(head.get(4..), Some(b"ftyp" | b"moov" | b"wide" | b"free")) {
        mp4(r, 0, r.len(), 0, false, &mut budget)
    } else {
        Ok(None)
    }
}
fn spend(budget: &mut usize) -> io::Result<()> {
    *budget = budget.checked_sub(1).ok_or_else(invalid)?;
    Ok(())
}
fn mp4(r: &dyn ByteReader, mut at: u64, end: u64, depth: u8, cover: bool, budget: &mut usize) -> io::Result<Option<Vec<u8>>> {
    if depth > 8 {
        return Err(invalid());
    }
    while at < end {
        spend(budget)?;
        if end.saturating_sub(at) < 8 {
            return Err(invalid());
        }
        let h = read::<8>(r, at)?;
        let len = u32::from_be_bytes(h.get(..4).ok_or_else(invalid)?.try_into().map_err(|_| invalid())?);
        let (len, header) = match len {
            0 => (end - at, 8),
            1 => (u64::from_be_bytes(read::<8>(r, at.checked_add(8).ok_or_else(invalid)?)?), 16),
            n => (u64::from(n), 8),
        };
        let next = at.checked_add(len).filter(|n| *n <= end && len >= header).ok_or_else(invalid)?;
        let start = at.checked_add(header).ok_or_else(invalid)?;
        let kind = h.get(4..).ok_or_else(invalid)?;
        if cover && kind == b"data" && next.saturating_sub(start) >= 8 {
            let flags = read::<8>(r, start)?;
            if matches!(flags.get(..4), Some([0, 0, 0, 13] | [0, 0, 0, 14])) {
                return payload(r, start.checked_add(8).ok_or_else(invalid)?, next).map(Some);
            }
        }
        if matches!(kind, b"moov" | b"udta" | b"meta" | b"ilst" | b"covr") {
            let child = start.checked_add(if kind == b"meta" { 4 } else { 0 }).filter(|p| *p <= next).ok_or_else(invalid)?;
            if let Some(bytes) = mp4(r, child, next, depth + 1, kind == b"covr", budget)? {
                return Ok(Some(bytes));
            }
        }
        at = next;
    }
    Ok(None)
}
fn vint(r: &dyn ByteReader, at: &mut u64, end: u64, id: bool) -> io::Result<(u64, bool)> {
    if *at >= end {
        return Err(invalid());
    }
    let first = read::<1>(r, *at)?;
    let first = *first.first().ok_or_else(invalid)?;
    let n = first.leading_zeros() + 1;
    if n > if id { 4 } else { 8 } {
        return Err(invalid());
    }
    let mut value = u64::from(if id { first } else { first & ((0xffu16 >> n) as u8) });
    *at = at.checked_add(1).ok_or_else(invalid)?;
    for _ in 1..n {
        if *at >= end {
            return Err(invalid());
        }
        value = (value << 8) | u64::from(*read::<1>(r, *at)?.first().ok_or_else(invalid)?);
        *at = at.checked_add(1).ok_or_else(invalid)?;
    }
    Ok((value, !id && value == (1u64 << (7 * n)) - 1))
}
fn element(r: &dyn ByteReader, at: &mut u64, end: u64, budget: &mut usize) -> io::Result<(u64, u64, u64)> {
    spend(budget)?;
    let (id, _) = vint(r, at, end, true)?;
    let (size, unknown) = vint(r, at, end, false)?;
    // Unknown-sized clusters cannot be skipped safely. A later cover is an optional cache miss.
    if unknown && id != 0x18538067 {
        return Err(invalid());
    }
    let next = if unknown { end } else { at.checked_add(size).filter(|n| *n <= end).ok_or_else(invalid)? };
    let start = *at;
    *at = next;
    Ok((id, start, next))
}
fn mkv(r: &dyn ByteReader, mut at: u64, end: u64, depth: u8, budget: &mut usize) -> io::Result<Option<Vec<u8>>> {
    if depth > 4 {
        return Err(invalid());
    }
    while at < end {
        let (id, start, next) = element(r, &mut at, end, budget)?;
        if id == 0x18538067 || id == 0x1941a469 {
            if let Some(bytes) = mkv(r, start, next, depth + 1, budget)? {
                return Ok(Some(bytes));
            }
        } else if id == 0x61a7 {
            let mut pos = start;
            let mut name = None;
            let mut data = None;
            while pos < next {
                let (id, a, b) = element(r, &mut pos, next, budget)?;
                if id == 0x466e && b.saturating_sub(a) <= 256 {
                    name = Some(payload(r, a, b)?);
                }
                if id == 0x465c {
                    data = Some((a, b));
                }
            }
            if name.as_deref().is_some_and(|name| {
                matches!(
                    name,
                    b"cover.jpg"
                        | b"cover.jpeg"
                        | b"cover.png"
                        | b"cover_land.jpg"
                        | b"cover_land.png"
                        | b"small_cover.jpg"
                        | b"small_cover.png"
                        | b"small_cover_land.jpg"
                        | b"small_cover_land.png"
                )
            }) && let Some((a, b)) = data
            {
                return payload(r, a, b).map(Some);
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::MemReader;
    fn atom(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut b = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend(kind);
        b.extend(data);
        b
    }
    fn mp4_cover() -> Vec<u8> {
        let mut data = vec![0, 0, 0, 14, 0, 0, 0, 0];
        data.extend(b"image bytes");
        let covr = atom(b"covr", &atom(b"data", &data));
        let mut meta = vec![0; 4];
        meta.extend(atom(b"ilst", &covr));
        let mut file = atom(b"ftyp", b"isom0000");
        file.extend(atom(b"mdat", &[0; 128]));
        file.extend(atom(b"moov", &atom(b"udta", &atom(b"meta", &meta))));
        file
    }
    fn elem(id: &[u8], data: &[u8]) -> Vec<u8> {
        assert!(data.len() < 127);
        let mut b = id.to_vec();
        b.push(0x80 | data.len() as u8);
        b.extend(data);
        b
    }
    fn mkv_cover() -> Vec<u8> {
        // The data deliberately precedes its filename; attachment field order is not fixed.
        let mut attached = elem(&[0x46, 0x5c], b"image bytes");
        attached.extend(elem(&[0x46, 0x6e], b"cover.png"));
        let attachments = elem(&[0x19, 0x41, 0xa4, 0x69], &elem(&[0x61, 0xa7], &attached));
        let mut bytes = elem(&[0x1a, 0x45, 0xdf, 0xa3], &[]);
        bytes.extend([0x18, 0x53, 0x80, 0x67, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        bytes.extend(attachments);
        bytes
    }
    #[test]
    fn extracts_covers_without_opening_video_tracks() {
        for bytes in [mp4_cover(), mkv_cover()] {
            assert_eq!(embedded(&MemReader(bytes.into())).unwrap().unwrap(), b"image bytes");
        }
    }
    #[test]
    fn malformed_sizes_truncation_and_bit_flips_never_panic() {
        for bytes in [mp4_cover(), mkv_cover()] {
            for end in 0..bytes.len() {
                let reader = MemReader(bytes[..end].to_vec().into());
                assert!(std::panic::catch_unwind(|| embedded(&reader)).is_ok());
            }
            for at in 0..bytes.len() {
                for mask in [1, 0x80, 0xff] {
                    let mut mutated = bytes.clone();
                    mutated[at] ^= mask;
                    let reader = MemReader(mutated.into());
                    assert!(std::panic::catch_unwind(|| embedded(&reader)).is_ok());
                }
            }
        }
        let mut huge = atom(b"moov", &[]);
        huge[..4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(embedded(&MemReader(huge.into())).is_err());
        let mut deep = Vec::new();
        for _ in 0..12 {
            deep = atom(b"moov", &deep);
        }
        assert!(embedded(&MemReader(deep.into())).is_err());
    }
}
