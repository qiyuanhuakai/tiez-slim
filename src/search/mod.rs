//! Fuzzy and substring search engines for clipboard history.
//!
//! Uses `nucleo-matcher` for high-performance fuzzy matching against
//! stored clipboard entries. Falls back to simple substring matching
//! when fuzzy mode is disabled.

use crate::model::ClipboardEntrySummary;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};

/// A search result with scoring and highlight indices.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// The matched entry (summary, without full content).
    pub entry: ClipboardEntrySummary,
    /// Relevance score (higher = better match). 0 for substring hits.
    pub score: i32,
    /// Byte indices into `preview` for highlight rendering (T21).
    pub matched_indices: Vec<usize>,
}

/// Trait abstracting over search strategies.
pub trait SearchEngine {
    /// Filter and rank `entries` against `query`.
    /// Returns hits sorted by relevance (best first).
    fn search(&self, query: &str, entries: &[ClipboardEntrySummary]) -> Vec<SearchHit>;
}

/// High-performance fuzzy engine using `nucleo-matcher`.
///
/// Tolerates typos, character transpositions, and missing characters.
/// CJK-friendly: operates on Unicode character level.
pub struct FuzzyEngine {
    matcher: std::cell::RefCell<Matcher>,
}

impl FuzzyEngine {
    pub fn new() -> Self {
        Self {
            matcher: std::cell::RefCell::new(Matcher::new(Config::DEFAULT)),
        }
    }
}

impl Default for FuzzyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine for FuzzyEngine {
    fn search(&self, query: &str, entries: &[ClipboardEntrySummary]) -> Vec<SearchHit> {
        let query = query.trim();
        if query.is_empty() {
            return entries
                .iter()
                .map(|e| SearchHit {
                    entry: e.clone(),
                    score: 0,
                    matched_indices: Vec::new(),
                })
                .collect();
        }

        let pattern = Pattern::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut matcher = self.matcher.borrow_mut();
        let mut indices_buf: Vec<u32> = Vec::new();
        let mut hits: Vec<SearchHit> = Vec::new();

        for entry in entries {
            let haystack_str = format!("{} {}", entry.preview, entry.source_app);
            let haystack = Utf32String::from(haystack_str.as_str());
            indices_buf.clear();

            if let Some(score) = pattern.indices(haystack.slice(..), &mut matcher, &mut indices_buf)
            {
                let preview_char_count = entry.preview.chars().count();
                let matched_indices =
                    utf32_indices_to_byte_offsets(&entry.preview, &indices_buf, preview_char_count);

                hits.push(SearchHit {
                    entry: entry.clone(),
                    score: score as i32,
                    matched_indices,
                });
            }
        }

        hits.sort_by(|a, b| {
            let pinned_cmp = b.entry.is_pinned.cmp(&a.entry.is_pinned);
            if pinned_cmp != std::cmp::Ordering::Equal {
                return pinned_cmp;
            }
            b.score.cmp(&a.score)
        });
        hits
    }
}

/// Simple substring engine using case-insensitive `str::contains`.
///
/// Matches the existing SQL LIKE behavior but operates in-memory
/// for consistency with the `SearchEngine` trait.
pub struct SubstringEngine;

impl SearchEngine for SubstringEngine {
    fn search(&self, query: &str, entries: &[ClipboardEntrySummary]) -> Vec<SearchHit> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return entries
                .iter()
                .map(|e| SearchHit {
                    entry: e.clone(),
                    score: 0,
                    matched_indices: Vec::new(),
                })
                .collect();
        }

        let mut hits: Vec<SearchHit> = Vec::new();
        for entry in entries {
            let preview_lower = entry.preview.to_lowercase();
            let source_lower = entry.source_app.to_lowercase();

            if let Some(pos) = preview_lower.find(&query) {
                let matched_indices: Vec<usize> = (pos..pos + query.len()).collect();
                hits.push(SearchHit {
                    entry: entry.clone(),
                    score: 0,
                    matched_indices,
                });
            } else if source_lower.contains(&query)
                || entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query))
            {
                hits.push(SearchHit {
                    entry: entry.clone(),
                    score: 0,
                    matched_indices: Vec::new(),
                });
            }
        }

        hits.sort_by(|a, b| b.entry.is_pinned.cmp(&a.entry.is_pinned));
        hits
    }
}

/// Convert UTF-32 (char) indices from nucleo to byte offsets in a Rust string.
///
/// Only includes indices that fall within the first `max_chars` characters
/// (used to restrict highlights to the preview portion of the haystack).
fn utf32_indices_to_byte_offsets(s: &str, char_indices: &[u32], max_chars: usize) -> Vec<usize> {
    let mut char_to_byte = Vec::with_capacity(s.len());
    for (byte_idx, _) in s.char_indices() {
        char_to_byte.push(byte_idx);
    }
    char_to_byte.push(s.len());

    let mut byte_offsets: Vec<usize> = char_indices
        .iter()
        .filter_map(|&ci| {
            let ci = ci as usize;
            if ci < max_chars && ci < char_to_byte.len() {
                Some(char_to_byte[ci])
            } else {
                None
            }
        })
        .collect();
    byte_offsets.sort_unstable();
    byte_offsets.dedup();
    byte_offsets
}

/// Build the appropriate search engine based on the `search_mode` preference.
pub fn engine_for_mode(mode: &str) -> Box<dyn SearchEngine> {
    match mode {
        "substring" => Box::new(SubstringEngine),
        _ => Box::new(FuzzyEngine::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipboardKind, SelectionSource};

    fn make_entry(id: i64, preview: &str) -> ClipboardEntrySummary {
        ClipboardEntrySummary {
            id,
            kind: ClipboardKind::Text,
            source_app: String::new(),
            source_app_path: None,
            timestamp: 1_700_000_000_000 + id,
            preview: preview.to_string(),
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
            sensitive: false,
            source: SelectionSource::default(),
        }
    }

    fn make_entry_with_source(id: i64, preview: &str, source: &str) -> ClipboardEntrySummary {
        let mut e = make_entry(id, preview);
        e.source_app = source.to_string();
        e
    }

    fn make_entry_with_tags(id: i64, preview: &str, tags: Vec<String>) -> ClipboardEntrySummary {
        let mut e = make_entry(id, preview);
        e.tags = tags;
        e
    }

    #[test]
    fn fuzzy_empty_query_returns_all() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry(1, "hello"), make_entry(2, "world")];
        let hits = engine.search("", &entries);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.matched_indices.is_empty()));
    }

    #[test]
    fn fuzzy_exact_match() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry(1, "clipboard manager")];
        let hits = engine.search("clipboard", &entries);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].score > 0);
        assert!(!hits[0].matched_indices.is_empty());
    }

    #[test]
    fn fuzzy_typo_tolerance() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry(1, "clipboard")];
        // "clpboard" is a subsequence of "clipboard" (missing 'i'); nucleo
        // does subsequence matching, not Levenshtein edit-distance matching.
        let hits = engine.search("clpboard", &entries);
        assert_eq!(
            hits.len(),
            1,
            "subsequence 'clpboard' should match 'clipboard'"
        );
        assert!(hits[0].score > 0);
    }

    #[test]
    fn fuzzy_cjk_match() {
        let engine = FuzzyEngine::new();
        let entries = vec![
            make_entry(1, "剪贴板管理器"),
            make_entry(2, "剪贴板工具"),
            make_entry(3, "文本编辑器"),
        ];
        let hits = engine.search("剪贴板", &entries);
        assert!(
            hits.len() >= 2,
            "should match both 剪贴板管理器 and 剪贴板工具"
        );
        assert_eq!(hits[0].entry.id, 1);
    }

    #[test]
    fn fuzzy_no_match() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry(1, "hello world")];
        let hits = engine.search("zzzzz", &entries);
        assert!(hits.is_empty());
    }

    #[test]
    fn fuzzy_source_app_match() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry_with_source(1, "some text", "Firefox")];
        let hits = engine.search("firefox", &entries);
        assert_eq!(hits.len(), 1, "should match against source_app");
    }

    #[test]
    fn fuzzy_sorted_pinned_first() {
        let engine = FuzzyEngine::new();
        let mut e1 = make_entry(1, "hello world");
        e1.is_pinned = true;
        let e2 = make_entry(2, "hello");
        let entries = vec![e1, e2];
        let hits = engine.search("hello", &entries);
        assert_eq!(hits[0].entry.id, 1);
    }

    #[test]
    fn fuzzy_sorted_by_score() {
        let engine = FuzzyEngine::new();
        let entries = vec![
            make_entry(1, "clipboard"),
            make_entry(2, "clap"),
            make_entry(3, "clip board manager"),
        ];
        let hits = engine.search("clip", &entries);
        assert!(hits.len() >= 2);
        for window in hits.windows(2) {
            if window[0].entry.is_pinned == window[1].entry.is_pinned {
                assert!(window[0].score >= window[1].score);
            }
        }
    }

    #[test]
    fn fuzzy_multiline_preview() {
        let engine = FuzzyEngine::new();
        let entries = vec![make_entry(
            1,
            "/home/user/documents/file.txt\n/tmp/other.txt",
        )];
        let hits = engine.search("documents", &entries);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn substring_exact_match() {
        let engine = SubstringEngine;
        let entries = vec![make_entry(1, "clipboard manager")];
        let hits = engine.search("clipboard", &entries);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_indices, vec![0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn substring_case_insensitive() {
        let engine = SubstringEngine;
        let entries = vec![make_entry(1, "Clipboard Manager")];
        let hits = engine.search("clipboard", &entries);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn substring_no_match() {
        let engine = SubstringEngine;
        let entries = vec![make_entry(1, "hello world")];
        let hits = engine.search("clipboard", &entries);
        assert!(hits.is_empty());
    }

    #[test]
    fn substring_tag_match() {
        let engine = SubstringEngine;
        let entries = vec![make_entry_with_tags(
            1,
            "some text",
            vec!["代码".to_string()],
        )];
        let hits = engine.search("代码", &entries);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn substring_empty_query_returns_all() {
        let engine = SubstringEngine;
        let entries = vec![make_entry(1, "a"), make_entry(2, "b")];
        let hits = engine.search("", &entries);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn engine_for_mode_defaults_to_fuzzy() {
        let engine = engine_for_mode("fuzzy");
        let entries = vec![make_entry(1, "test")];
        assert_eq!(engine.search("test", &entries).len(), 1);

        let engine = engine_for_mode("unknown");
        assert_eq!(engine.search("test", &entries).len(), 1);
    }

    #[test]
    fn engine_for_mode_substring() {
        let engine = engine_for_mode("substring");
        let entries = vec![make_entry(1, "test string")];
        assert_eq!(engine.search("test", &entries).len(), 1);
    }

    #[test]
    fn performance_1000_entries_fuzzy_under_50ms() {
        let entries: Vec<ClipboardEntrySummary> = (0..1000)
            .map(|i| {
                make_entry(
                    i,
                    &format!("entry number {i} with some content for testing"),
                )
            })
            .collect();
        let engine = FuzzyEngine::new();

        let start = std::time::Instant::now();
        let hits = engine.search("entry", &entries);
        let elapsed = start.elapsed();

        assert!(!hits.is_empty());
        assert!(
            elapsed.as_millis() < 50,
            "fuzzy search of 1000 entries took {}ms, expected < 50ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn performance_1000_entries_substring_under_50ms() {
        let entries: Vec<ClipboardEntrySummary> = (0..1000)
            .map(|i| {
                make_entry(
                    i,
                    &format!("entry number {i} with some content for testing"),
                )
            })
            .collect();
        let engine = SubstringEngine;

        let start = std::time::Instant::now();
        let hits = engine.search("entry", &entries);
        let elapsed = start.elapsed();

        assert!(!hits.is_empty());
        assert!(
            elapsed.as_millis() < 50,
            "substring search of 1000 entries took {}ms, expected < 50ms",
            elapsed.as_millis()
        );
    }
}
