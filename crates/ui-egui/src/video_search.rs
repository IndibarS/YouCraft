//! Bounded, Unicode-aware title matching for the local video library.
const MAX_QUERY: usize = 128;
const MAX_TITLE: usize = 512;

fn words(text: &str, limit: usize) -> Vec<String> {
    let normalized: String = text.chars().take(limit).flat_map(char::to_lowercase).map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    normalized.split_whitespace().take(32).map(str::to_owned).collect()
}

/// Lower scores rank first; absent means the title does not match all query terms.
pub fn score(title: &str, query: &str) -> Option<usize> {
    let query_words = words(query, MAX_QUERY);
    if query_words.is_empty() {
        return Some(0);
    }
    let title_words = words(title, MAX_TITLE);
    let query = query_words.join(" ");
    let title = title_words.join(" ");
    if title == query {
        return Some(0);
    }
    if title.starts_with(&query) {
        return Some(1);
    }
    if title.contains(&query) {
        return Some(2);
    }
    if title_words.concat().contains(&query_words.concat()) {
        return Some(3);
    }
    let mut total = 10;
    for term in query_words {
        let q: Vec<char> = term.chars().collect();
        let allowance = match q.len() {
            0..=3 => 0,
            4..=7 => 1,
            _ => 2,
        };
        let best = title_words
            .iter()
            .filter_map(|word| {
                if word == &term {
                    return Some(0);
                }
                if word.starts_with(&term) {
                    return Some(1);
                }
                if allowance == 0 {
                    return None;
                }
                let target: Vec<char> = word.chars().take(MAX_QUERY + 2).collect();
                // Compare nearby prefix lengths so incomplete typing can still contain a typo.
                let start = q.len().saturating_sub(allowance).max(1);
                let end = target.len().min(q.len() + allowance);
                let distance = (start..=end).map(|n| edit_distance(&q, &target[..n])).min()?;
                (distance <= allowance).then_some(4 + distance * 4)
            })
            .min()?;
        total += best;
    }
    Some(total)
}

/// Optimal-string-alignment distance: insert, delete, replace, or swap adjacent characters.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut before_previous = previous.clone();
    let mut current = vec![0; b.len() + 1];
    for (i, left) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            current[j + 1] = (previous[j + 1] + 1).min(current[j] + 1).min(previous[j] + usize::from(left != right));
            if i > 0 && j > 0 && *left == b[j - 1] && a[i - 1] == *right {
                current[j + 1] = current[j + 1].min(before_previous[j - 1] + 1);
            }
        }
        std::mem::swap(&mut before_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}
