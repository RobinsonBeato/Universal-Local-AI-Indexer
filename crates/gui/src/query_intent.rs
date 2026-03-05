#[derive(Debug, Clone, Default)]
pub struct IntentParseResult {
    pub query: String,
    pub regex: Option<String>,
}

pub fn parse_natural_query(input: &str) -> IntentParseResult {
    let raw = input.trim();
    if raw.is_empty() {
        return IntentParseResult::default();
    }

    let lower = raw.to_lowercase();
    let quoted = extract_quoted(raw);
    let contains_phrase = extract_contains_phrase(raw, &lower);

    let query = quoted
        .or(contains_phrase)
        .unwrap_or_else(|| raw.to_string())
        .trim()
        .to_string();

    let regex = detect_filetype_regex(&lower);

    IntentParseResult { query, regex }
}

fn extract_quoted(input: &str) -> Option<String> {
    let chars: Vec<char> = input.chars().collect();
    for quote in ['"', '\''] {
        let start = chars.iter().position(|c| *c == quote)?;
        let end_rel = chars[start + 1..].iter().position(|c| *c == quote)?;
        let end = start + 1 + end_rel;
        let out = chars[start + 1..end].iter().collect::<String>();
        if !out.trim().is_empty() {
            return Some(out);
        }
    }
    None
}

fn extract_contains_phrase(original: &str, lower: &str) -> Option<String> {
    const MARKERS: [&str; 11] = [
        "que digan",
        "que diga",
        "que contengan",
        "que contenga",
        "that contain",
        "that contains",
        "containing",
        "contains",
        "contain",
        "with",
        "con ",
    ];

    let mut best: Option<(usize, &str)> = None;
    for marker in MARKERS {
        if let Some(pos) = lower.find(marker) {
            match best {
                None => best = Some((pos, marker)),
                Some((prev_pos, _)) if pos < prev_pos => best = Some((pos, marker)),
                _ => {}
            }
        }
    }

    let (pos, marker) = best?;
    let start = pos + marker.len();
    if start >= original.len() {
        return None;
    }
    let phrase = original[start..].trim().trim_matches(':').trim();
    if phrase.is_empty() {
        None
    } else {
        Some(phrase.to_string())
    }
}

fn detect_filetype_regex(lower: &str) -> Option<String> {
    let has_any = |words: &[&str]| words.iter().any(|w| lower.contains(w));

    if has_any(&[" json ", "json", "archivo json", "file json", "json files"]) {
        return Some(String::from("(?i)\\.json$"));
    }
    if has_any(&[" pdf ", "pdfs", "archivo pdf", "pdf file"]) {
        return Some(String::from("(?i)\\.pdf$"));
    }
    if has_any(&[
        "docx",
        "doc ",
        "word",
        "documento",
        "documentos",
        "document",
        "documents",
    ]) {
        return Some(String::from("(?i)\\.(doc|docx|odt|rtf)$"));
    }
    if has_any(&[
        "imagen", "imagenes", "foto", "fotos", "image", "images", "picture", "pictures",
    ]) {
        return Some(String::from("(?i)\\.(png|jpe?g|gif|bmp|webp|tiff?|ico)$"));
    }
    if has_any(&["codigo", "code", "source code", "program"]) {
        return Some(String::from(
            "(?i)\\.(rs|js|ts|tsx|jsx|py|java|go|cs|cpp|h|hpp|html|css|json|toml|yaml|yml|xml|sql|sh|ps1)$",
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_natural_query;

    #[test]
    fn parses_spanish_json_request() {
        let out = parse_natural_query("dame los archivos json que digan factura");
        assert_eq!(out.query, "factura");
        assert_eq!(out.regex.as_deref(), Some("(?i)\\.json$"));
    }

    #[test]
    fn parses_english_pdf_request() {
        let out = parse_natural_query("find pdf files that contain contract");
        assert_eq!(out.query, "contract");
        assert_eq!(out.regex.as_deref(), Some("(?i)\\.pdf$"));
    }

    #[test]
    fn quoted_text_wins() {
        let out = parse_natural_query("dame json que digan \"monto total\"");
        assert_eq!(out.query, "monto total");
        assert_eq!(out.regex.as_deref(), Some("(?i)\\.json$"));
    }
}
