use crate::model::{normalize, AppEntry};

pub const MAX_RESULTS: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub entry_index: usize,
    pub score: u32,
}

/// Initial independent ranking: exact, prefix, substring, then ordered subsequence.
/// It does not implement or claim compatibility with ALTRun/WindMenu scoring.
#[derive(Debug, Default)]
pub struct SearchEngine {
    query: String,
}

impl SearchEngine {
    /// The fixed output bounds retained memory. No candidate strings are allocated here.
    pub fn search(&mut self, entries: &[AppEntry], query: &str, output: &mut Vec<SearchHit>) {
        self.query.clear();
        self.query
            .extend(query.trim().chars().flat_map(char::to_lowercase));
        output.clear();
        for (entry_index, entry) in entries.iter().enumerate() {
            let score = if self.query.is_empty() {
                Some(0)
            } else {
                entry
                    .keys
                    .iter()
                    .filter_map(|key| rank(key, &self.query))
                    .max()
            };
            let Some(score) = score else { continue };
            let hit = SearchHit { entry_index, score };
            let position = output
                .iter()
                .position(|old| old.score < score)
                .unwrap_or(output.len());
            if position < MAX_RESULTS {
                if output.len() == MAX_RESULTS {
                    output.pop();
                }
                output.insert(position, hit);
            }
        }
    }
}

fn rank(key: &str, query: &str) -> Option<u32> {
    if key == query {
        return Some(4_000);
    }
    if key.starts_with(query) {
        return Some(3_000);
    }
    if key.contains(query) {
        return Some(2_000);
    }
    let mut expected = query.chars();
    let mut current = expected.next()?;
    for character in key.chars() {
        if character == current {
            match expected.next() {
                Some(next) => current = next,
                None => return Some(1_000),
            }
        }
    }
    None
}

/// Shared with the controller so repeated EN_CHANGE notifications can be ignored.
pub fn normalized_query(text: &str) -> String {
    normalize(text)
}
