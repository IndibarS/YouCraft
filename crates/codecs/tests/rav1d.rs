#![cfg(not(target_arch = "wasm32"))]
use filmcraft_codecs::{VideoDecoder, rav1d_decoder::Rav1dDecoder};
use filmcraft_frame::PixelData;

fn packets(bytes: &[u8]) -> Vec<&[u8]> {
    assert_eq!(&bytes[..4], b"DKIF");
    let mut at = 32;
    let mut packets = Vec::new();
    while at < bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        packets.push(&bytes[at + 12..at + 12 + len]);
        at += 12 + len;
    }
    packets
}
fn decode(dec: &mut Rav1dDecoder, packets: &[&[u8]]) -> Vec<filmcraft_codecs::DecodedFrame> {
    let mut frames = vec![];
    for (i, packet) in packets.iter().enumerate() {
        frames.extend(dec.decode(packet, i as i64 * 1000 + 7).unwrap());
    }
    frames.extend(dec.flush());
    frames.sort_by_key(|f| f.pts);
    frames
}
#[test]
#[ignore = "requires generated AV1 fixtures in YOUCRAFT_AV1_FIXTURES"]
fn matches_oracle_across_bit_depths_monochrome_and_seeking() {
    let root = std::path::PathBuf::from(std::env::var_os("YOUCRAFT_AV1_FIXTURES").expect("fixture directory"));
    for name in ["eight", "ten", "twelve", "mono"] {
        let bytes = std::fs::read(root.join(format!("{name}.ivf"))).unwrap();
        let expected = std::fs::read(root.join(format!("{name}.yuv"))).unwrap();
        let packets = packets(&bytes);
        let mut dec = Rav1dDecoder::new(vec![]).unwrap();
        assert_eq!(dec.name(), "rav1d AV1");
        for _ in 0..2 {
            let frames = decode(&mut dec, &packets);
            assert_eq!(frames.len(), 24);
            let mut raw = vec![];
            for (i, f) in frames.iter().enumerate() {
                assert_eq!(f.pts, i as i64 * 1000 + 7, "delayed frame timestamp");
                assert_eq!((f.frame.width, f.frame.height), (96, 64));
                match &f.frame.data {
                    PixelData::Yuv8 { planes, .. } => {
                        if name == "mono" {
                            assert!(planes[1].iter().chain(planes[2].iter()).all(|v| *v == 128));
                            raw.extend_from_slice(&planes[0]);
                        } else {
                            for p in planes {
                                raw.extend_from_slice(p);
                            }
                        }
                    }
                    PixelData::Yuv16 { planes, bits, .. } => {
                        assert_eq!(*bits, if name == "twelve" { 12 } else { 10 });
                        for p in planes {
                            for v in p.iter() {
                                raw.extend_from_slice(&v.to_le_bytes());
                            }
                        }
                    }
                    _ => panic!("expected planar video"),
                }
            }
            assert_eq!(raw, expected, "decoded pixels must match FFmpeg oracle for {name}");
            dec.reset();
        }
        for packet in packets.iter().take(4) {
            let _ = dec.decode(packet, 1);
        }
        dec.reset();
        assert_eq!(decode(&mut dec, &packets).len(), 24, "seek discards pending old pictures");
        for len in [1, 2, 3, 8, 16] {
            let _ = dec.decode(&packets[0][..len.min(packets[0].len())], 0);
            dec.reset();
        }
        assert_eq!(decode(&mut dec, &packets).len(), 24, "malformed input does not poison later playback");
    }
}

#[test]
fn native_factory_selects_rav1d_and_thumbnails_use_limited_threads() {
    use filmcraft_isobmff::{Av1Config, CodecConfig, FourCc, SampleEntry};
    let entry = SampleEntry::video(FourCc(*b"av01"), CodecConfig::Av1(Av1Config::default()), 96, 64);
    let decoder = filmcraft_codecs::video::av1_factory(&entry).unwrap().unwrap();
    assert_eq!(decoder.name(), "rav1d AV1");
    assert!(!decoder.thread_limited());
    filmcraft_media::cancel::with_background(true, || {
        let decoder = filmcraft_codecs::video::av1_factory(&entry).unwrap().unwrap();
        assert!(decoder.thread_limited());
    });
}
