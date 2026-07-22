//! Office/xlsx 文本抽取：zip 解压 + 抽取 XML 文本节点。零新依赖（复用 zip）。
use std::io::Read;

/// 读取 zip 内某个 entry 的全部字节为字符串（不存在返回 None）。
fn read_entry(bytes: &[u8], name: &str) -> Result<Option<String>, String> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("非法的压缩包：{e}"))?;
    // 先判断是否存在，再读取，避免借用 zip 的生命周期跨越 match 结束。
    let exists = zip.by_name(name).is_ok();
    if !exists {
        return Ok(None);
    }
    let mut f = zip.by_name(name).unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(|e| format!("读取 {name} 失败：{e}"))?;
    Ok(Some(s))
}

/// 列出 zip 内匹配前缀且以 .xml 结尾的 entry 名（多 slide / 多 sheet）。
fn list_entries(bytes: &[u8], prefix: &str) -> Result<Vec<String>, String> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("非法的压缩包：{e}"))?;
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        let n = f.name().to_string();
        if n.starts_with(prefix) && n.ends_with(".xml") {
            names.push(n);
        }
    }
    names.sort();
    Ok(names)
}

/// 极简 XML→文本：段落结束标签换行，去掉所有标签，解码常见实体，折叠空行。
fn xml_to_text(xml: &str) -> String {
    let mut s = xml.to_string();
    for para_end in ["</w:p>", "</a:p>", "</p>", "</w:tr>"] {
        s = s.replace(para_end, "\n");
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    out.lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn extract_docx(bytes: &[u8]) -> Result<String, String> {
    let xml = read_entry(bytes, "word/document.xml")?
        .ok_or("不是有效的 docx（缺 document.xml）")?;
    let text = xml_to_text(&xml);
    if text.trim().is_empty() {
        Err("docx 未提取到文本".into())
    } else {
        Ok(text)
    }
}

pub fn extract_pptx(bytes: &[u8]) -> Result<String, String> {
    let slides = list_entries(bytes, "ppt/slides/slide")?;
    if slides.is_empty() {
        return Err("不是有效的 pptx（无 slide）".into());
    }
    let mut parts = Vec::new();
    for name in slides {
        if let Some(xml) = read_entry(bytes, &name)? {
            let t = xml_to_text(&xml);
            if !t.trim().is_empty() {
                parts.push(t);
            }
        }
    }
    if parts.is_empty() {
        Err("pptx 未提取到文本".into())
    } else {
        Ok(parts.join("\n\n"))
    }
}

pub fn extract_xlsx(bytes: &[u8]) -> Result<String, String> {
    let shared = read_entry(bytes, "xl/sharedStrings.xml")?
        .map(|x| xml_to_text(&x))
        .unwrap_or_default();
    let sheets = list_entries(bytes, "xl/worksheets/sheet")?;
    let mut parts = Vec::new();
    if !shared.trim().is_empty() {
        parts.push(shared);
    }
    for name in sheets {
        if let Some(xml) = read_entry(bytes, &name)? {
            let t = xml_to_text(&xml);
            if !t.trim().is_empty() {
                parts.push(t);
            }
        }
    }
    if parts.is_empty() {
        Err("xlsx 未提取到文本".into())
    } else {
        Ok(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 用 zip 现场打一个最小压缩包。
    fn build_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, body) in entries {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(body.as_bytes()).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn docx_extracts_paragraph_text() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>第一段</w:t></w:r></w:p><w:p><w:r><w:t>第二段</w:t></w:r></w:p></w:body></w:document>"#;
        let bytes = build_zip(&[("word/document.xml", xml)]);
        let text = extract_docx(&bytes).unwrap();
        assert!(text.contains("第一段"));
        assert!(text.contains("第二段"));
        assert!(text.lines().count() >= 2);
    }

    #[test]
    fn xlsx_extracts_shared_strings() {
        let shared = r#"<sst><si><t>销售额</t></si><si><t>区域</t></si></sst>"#;
        let sheet = r#"<worksheet><sheetData></sheetData></worksheet>"#;
        let bytes = build_zip(&[
            ("xl/sharedStrings.xml", shared),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let text = extract_xlsx(&bytes).unwrap();
        assert!(text.contains("销售额"));
        assert!(text.contains("区域"));
    }

    #[test]
    fn corrupt_zip_errors() {
        assert!(extract_docx(b"not a zip").is_err());
    }
}
