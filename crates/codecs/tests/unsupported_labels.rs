#[test]
fn unsupported_mpeg4_part2_names_the_codec_instead_of_unknown() {
    use filmcraft_isobmff::{CodecConfig, FourCc, SampleEntry};
    let fourcc = FourCc(*b"mp4v");
    let entry = SampleEntry::video(fourcc, CodecConfig::Unknown { fourcc, raw: vec![] }, 320, 240);
    let error = match filmcraft_codecs::software_video_decoder(&entry) {
        Ok(_) => panic!("test expects MPEG-4 Part 2 to remain explicitly unsupported"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("MPEG-4 Part 2 (mp4v)"), "{error}");
}
