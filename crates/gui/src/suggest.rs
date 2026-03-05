use std::collections::HashSet;

pub fn build_suggestions(
    input: &str,
    recent_queries: &[String],
    context_terms: &[String],
) -> Vec<String> {
    let needle = input.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return recent_queries.iter().take(8).cloned().collect();
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // 1) Recent queries first (so selected terms stay on top).
    for q in recent_queries {
        let ql = q.to_ascii_lowercase();
        if (ql.starts_with(&needle) || ql.contains(&needle)) && seen.insert(q.clone()) {
            out.push(q.clone());
        }
        if out.len() >= 8 {
            return out;
        }
    }

    // 2) Natural-language intent templates (ES/EN).
    for t in intent_templates() {
        if t.to_ascii_lowercase().contains(&needle) && seen.insert(t.to_string()) {
            out.push(t.to_string());
        }
        if out.len() >= 8 {
            return out;
        }
    }

    // 3) Context terms from last results.
    for t in context_terms {
        let tl = t.to_ascii_lowercase();
        if (tl.starts_with(&needle) || tl.contains(&needle)) && seen.insert(t.clone()) {
            out.push(t.clone());
        }
        if out.len() >= 8 {
            return out;
        }
    }

    out
}

fn intent_templates() -> &'static [&'static str] {
    &[
        "dame los archivos json que digan ",
        "dame los pdf que contengan ",
        "dame documentos de word con ",
        "find json files that contain ",
        "find pdf files with ",
        "find word documents containing ",
        "imagenes que contengan ",
        "images containing ",
    ]
}

#[cfg(test)]
mod tests {
    use super::build_suggestions;

    #[test]
    fn suggests_templates_and_recent() {
        let recent = vec!["factura ute".to_string(), "json de config".to_string()];
        let ctx = vec!["invoice".to_string(), "contract".to_string()];
        let out = build_suggestions("json", &recent, &ctx);
        assert!(!out.is_empty());
        assert!(out.iter().any(|s| s.to_ascii_lowercase().contains("json")));
    }
}
