use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

pub fn extract_docx_text(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("no se pudo abrir docx {}", path.display()))?;
    let mut zip =
        ZipArchive::new(file).with_context(|| format!("docx inválido (zip) {}", path.display()))?;

    let mut xml_parts = (0..zip.len())
        .filter_map(|i| {
            let name = zip.by_index(i).ok()?.name().to_string();
            let is_word_xml = name.starts_with("word/") && name.ends_with(".xml");
            if is_word_xml {
                Some(name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    xml_parts.sort();

    let mut out = String::new();
    for name in xml_parts {
        let mut entry = zip
            .by_name(&name)
            .with_context(|| format!("entrada docx inválida: {name}"))?;
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .with_context(|| format!("no se pudo leer xml interno {name}"))?;

        let text = extract_text_from_xml(&xml)?;
        if !text.trim().is_empty() {
            out.push_str(&text);
            out.push('\n');
        }
    }

    Ok(out.trim().to_string())
}

#[cfg(feature = "pdf")]
pub fn extract_pdf_text(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path)
        .with_context(|| format!("no se pudo extraer texto pdf {}", path.display()))
}

#[cfg(not(feature = "pdf"))]
pub fn extract_pdf_text(_path: &Path) -> Result<String> {
    Ok(String::new())
}

fn extract_text_from_xml(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                let text = e.decode().map(|cow| cow.into_owned()).unwrap_or_default();
                if !text.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&text);
                }
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                if !text.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err.into()),
        }
        buf.clear();
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::extract_docx_text;

    #[test]
    fn extracts_text_from_minimal_docx() {
        let path = std::env::temp_dir().join(format!(
            "lupa_docx_test_{}.docx",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch should be available")
                .as_nanos()
        ));

        let file = std::fs::File::create(&path).expect("should create temp docx");
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", opts)
            .expect("should create document.xml");
        zip.write_all(br#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>hola docx</w:t></w:r></w:p></w:body></w:document>"#)
            .expect("should write xml");
        zip.finish().expect("should finish zip");

        let text = extract_docx_text(&path).expect("should extract docx text");
        assert!(text.contains("hola docx"));

        let _ = std::fs::remove_file(path);
    }
}
