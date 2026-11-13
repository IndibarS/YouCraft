#[path = "../src/video_search.rs"]
mod video_search;
use video_search::score;

#[test]
fn tolerates_common_typing_errors_and_partial_words() {
    for query in ["frankenstien", "franknstein", "frankkenstein", "frankenatein", "franknst", "franken stein"] {
        assert!(score("Frankenstein (2025)", query).is_some(), "{query}");
    }
    assert!(score("Ocean Sunset", "ocena suns").is_some());
}
#[test]
fn ranks_exact_matches_and_handles_word_order_and_separators() {
    assert!(score("Ocean Sunset", "ocean").unwrap() < score("Oceans Sunset", "ocena").unwrap());
    for query in ["CITY night", "night city", "citynight", "city-night"] {
        assert!(score("City_Night_Drive", query).is_some(), "{query}");
    }
    assert!(score("Ocean Sunset", "ocean unrelated").is_none());
    assert!(score("Ocean Sunset", "no-such-video-name").is_none());
}
#[test]
fn middle_edits_and_unicode_are_recomputed() {
    for query in ["frankn", "franken", "frankenstien", "frankenstein"] {
        assert!(score("Frankenstein", query).is_some());
    }
    assert!(score("বাংলা চলচ্চিত্র", "বাংলা").is_some());
    assert!(score("ÉTÉ à Paris", "été paris").is_some());
    assert!(score("Ocean Sunset", "").is_some());
    assert!(score("Ocean Sunset", "ox").is_none());
    let _ = score(&"界".repeat(10_000), &"字".repeat(10_000));
}
