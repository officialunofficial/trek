//! Count how many markdown fixtures Trek produces a byte-exact match for.
//!
//! Used during the Track B refactor; not part of the test harness.

use std::fs;
use std::path::PathBuf;

use trek_rs::{Trek, TrekOptions};

fn main() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_dir = crate_root.join("tests/fixtures");
    let expected_dir = crate_root.join("tests/expected");

    let frontmatter = regex::Regex::new(r#"<!--\s*(\{"url":.*?\})\s*-->"#).unwrap();
    let prefix_pat = regex::Regex::new(r"^[a-z]+--").unwrap();

    let mut entries: Vec<_> = fs::read_dir(&fixtures_dir)
        .expect("read fixtures dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("html"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut total = 0usize;
    let mut pass = 0usize;
    let mut body_pass = 0usize;
    let mut body_pass_names: Vec<String> = Vec::new();
    let mut failing: Vec<(String, String, String)> = Vec::new();
    // Fixtures whose markdown body matches Defuddle but whose JSON metadata
    // preamble does not — i.e. pure metadata-mismatch cases. Useful when
    // tuning metadata heuristics: list with `BODY_ONLY=1`.
    let mut body_only: Vec<String> = Vec::new();

    for entry in entries {
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let html = fs::read_to_string(&path).expect("read fixture");

        // Resolve URL.
        let url = if let Some(caps) = frontmatter.captures(&html) {
            let parsed: serde_json::Value = serde_json::from_str(&caps[1]).unwrap_or_default();
            parsed
                .get("url")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_default()
        } else {
            let stripped = prefix_pat.replace(&stem, "").to_string();
            format!("https://{stripped}")
        };

        let mut options = TrekOptions::default();
        options.url = Some(url);
        options.output.separate_markdown = true;
        let trek = Trek::new(options);
        let response = match trek.parse(&html) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut map = serde_json::Map::new();
        map.insert(
            "title".into(),
            serde_json::Value::String(response.metadata.title.clone()),
        );
        map.insert(
            "author".into(),
            serde_json::Value::String(response.metadata.author.clone()),
        );
        map.insert(
            "site".into(),
            serde_json::Value::String(response.metadata.site.clone()),
        );
        map.insert(
            "published".into(),
            serde_json::Value::String(response.metadata.published.clone()),
        );
        let json = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap();
        let body = response.content_markdown.clone().unwrap_or_default();
        let result = format!("```json\n{json}\n```\n\n{body}");

        let expected_path = expected_dir.join(format!("{stem}.md"));
        let Ok(expected) = fs::read_to_string(&expected_path) else {
            continue;
        };
        total += 1;
        if result.trim() == expected.trim() {
            pass += 1;
        } else {
            failing.push((stem.clone(), expected.clone(), result.clone()));
        }

        // Body-only comparison: strip the front JSON code fence if present.
        let strip_front_json = |s: &str| -> String {
            let trimmed = s.trim_start();
            if let Some(rest) = trimmed.strip_prefix("```json\n") {
                if let Some(end) = rest.find("\n```\n") {
                    return rest[end + 5..].trim().to_string();
                }
                if let Some(end) = rest.find("\n```") {
                    return rest[end + 4..].trim().to_string();
                }
            }
            trimmed.trim().to_string()
        };
        let body_actual = strip_front_json(&result);
        let body_expected = strip_front_json(&expected);
        if body_actual == body_expected {
            body_pass += 1;
            if result.trim() != expected.trim() {
                body_only.push(stem.clone());
            }
        }
    }

    println!("PASS: {pass}/{total}");
    println!("BODY: {body_pass}/{total}");
    if std::env::var("LIST_BODY_PASS").is_ok() {
        for n in &body_pass_names {
            println!("BODY-PASS: {n}");
        }
    }

    if std::env::var("LIST_PASS").is_ok() {
        let failing_names: std::collections::HashSet<_> =
            failing.iter().map(|(n, _, _)| n.clone()).collect();
        for entry in fs::read_dir(&fixtures_dir)
            .expect("read")
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap()
                .to_string();
            if expected_dir.join(format!("{stem}.md")).exists() && !failing_names.contains(&stem) {
                println!("PASS: {stem}");
            }
        }
    }

    if let Ok(target) = std::env::var("ONE") {
        for (name, expected, actual) in &failing {
            if name == &target {
                println!("\n=== FAIL: {name} ===");
                let e_lines: Vec<&str> = expected.trim().lines().collect();
                let a_lines: Vec<&str> = actual.trim().lines().collect();
                for i in 0..e_lines.len().max(a_lines.len()) {
                    let e = e_lines.get(i).copied().unwrap_or("");
                    let a = a_lines.get(i).copied().unwrap_or("");
                    if e == a {
                        println!("  {e}");
                    } else {
                        println!("- {e}");
                        println!("+ {a}");
                    }
                }
                return;
            }
        }
        println!("not found in failures");
        return;
    }
    if let Ok(diff_n) = std::env::var("DIFF_N") {
        let n: usize = diff_n.parse().unwrap_or(3);
        for (name, expected, actual) in failing.iter().take(n) {
            println!("\n=== FAIL: {name} ===");
            println!("--- expected ---\n{}\n", expected.trim());
            println!("--- actual ---\n{}\n", actual.trim());
        }
    } else if std::env::var("SMALL_DIFFS").is_ok() {
        // Sort by absolute character difference, ascending.
        let mut small = failing.clone();
        small.sort_by_key(|(_, e, a)| {
            ((a.trim().len() as isize - e.trim().len() as isize).abs()) as usize
        });
        let take = std::env::var("SMALL_N")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        for (name, expected, actual) in small.iter().take(take) {
            println!(
                "\n=== FAIL: {name} (Δlen={}) ===",
                actual.len() as isize - expected.len() as isize
            );
            println!("--- expected ---\n{}\n", expected.trim());
            println!("--- actual ---\n{}\n", actual.trim());
        }
    } else if std::env::var("LIST_FAILS").is_ok() {
        for (name, _, _) in &failing {
            println!("FAIL: {name}");
        }
    } else if std::env::var("BODY_ONLY").is_ok() {
        for name in &body_only {
            println!("BODY_ONLY: {name}");
        }
    }
}
