//! Content scoring algorithm for Trek

#![allow(clippy::cast_precision_loss)]

use crate::constants::{CONTENT_INDICATORS, NAVIGATION_INDICATORS, NON_CONTENT_PATTERNS};
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::{debug, instrument};

// Regex patterns
static DATE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}\b")
        .expect("Invalid regex")
});
static AUTHOR_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:by|written by|author:)\s+[A-Za-z\s]+\b").expect("Invalid regex")
});
static PARAGRAPH_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<p[^>]*>.*?</p>").expect("Invalid regex"));
static LINK_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<a[^>]*>.*?</a>").expect("Invalid regex"));
static IMAGE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"<img[^>]*>").expect("Invalid regex"));

/// Score for content elements
#[derive(Debug, Clone)]
pub struct ContentScore {
    pub score: f64,
    pub element_id: String,
}

/// Content scoring functionality
pub struct ContentScorer;

impl ContentScorer {
    /// Score text content based on various heuristics
    #[instrument(skip(text))]
    pub fn score_text(text: &str) -> f32 {
        let mut score = 0.0;

        // Text density
        let word_count = text.split_whitespace().count();
        let word_count_f32 = word_count as f32;
        score += word_count_f32;

        // Paragraph ratio
        let paragraphs = PARAGRAPH_PATTERN.find_iter(text).count();
        let paragraphs_f32 = paragraphs as f32;
        if paragraphs > 0 {
            score += paragraphs_f32 * 5.0;
        }

        // Link density penalty
        let links = LINK_PATTERN.find_iter(text).count();
        let links_f32 = links as f32;
        if word_count > 0 {
            let link_density = links_f32 / word_count_f32;
            if link_density > 0.5 {
                score *= 0.5;
            }
        }

        // Image bonus
        let images = IMAGE_PATTERN.find_iter(text).count();
        let images_f32 = images as f32;
        score += images_f32 * 3.0;

        // Content indicators bonus
        for indicator in CONTENT_INDICATORS {
            if text.contains(indicator) {
                score += 10.0;
            }
        }

        // Navigation indicators penalty
        for indicator in NAVIGATION_INDICATORS {
            if text.contains(indicator) {
                score -= 20.0;
            }
        }

        // Non-content patterns penalty
        for pattern in NON_CONTENT_PATTERNS {
            if text.contains(pattern) {
                score -= 30.0;
            }
        }

        // Date and author bonus
        if DATE_PATTERN.is_match(text) {
            score += 5.0;
        }
        if AUTHOR_PATTERN.is_match(text) {
            score += 5.0;
        }

        debug!("Scored content with {} words: {}", word_count, score);

        score.max(0.0)
    }

    /// Score based on HTML attributes
    pub fn score_by_attributes(tag: &str, class: Option<&str>, id: Option<&str>) -> f32 {
        let mut score = 0.0;

        // Tag-based scoring
        match tag {
            "article" | "main" => score += 20.0,
            "section" => score += 10.0,
            "div" => score += 5.0,
            "nav" | "aside" | "footer" | "header" => score -= 20.0,
            _ => {}
        }

        // Class-based scoring
        if let Some(class_str) = class {
            let class_lower = class_str.to_lowercase();

            // Content indicators
            if class_lower.contains("content")
                || class_lower.contains("article")
                || class_lower.contains("post")
                || class_lower.contains("entry")
            {
                score += 15.0;
            }

            // Navigation indicators
            if class_lower.contains("nav")
                || class_lower.contains("menu")
                || class_lower.contains("sidebar")
                || class_lower.contains("comment")
            {
                score -= 15.0;
            }
        }

        // ID-based scoring
        if let Some(id_str) = id {
            let id_lower = id_str.to_lowercase();

            if id_lower.contains("content") || id_lower.contains("main") {
                score += 10.0;
            }

            if id_lower.contains("nav") || id_lower.contains("sidebar") {
                score -= 10.0;
            }
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_text() {
        let text = r"
            <p>This is a paragraph with some content.</p>
            <p>Another paragraph with more text.</p>
        ";

        let score = ContentScorer::score_text(text);
        assert!(score > 0.0);
    }

    #[test]
    fn test_score_by_attributes() {
        let score = ContentScorer::score_by_attributes("article", Some("post-content"), None);
        assert!(score > 20.0);

        let nav_score = ContentScorer::score_by_attributes("nav", None, None);
        assert!(nav_score < 0.0);
    }
}
