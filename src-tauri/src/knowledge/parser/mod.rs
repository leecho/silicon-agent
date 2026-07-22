//! 文件解析：按扩展名把字节抽取为纯文本。Office/xlsx 用 zip+XML，PDF 用 pdf-extract。
//! URL 文本不在此（见 ingest::ingest_url 复用 web_fetch）。

mod office;
mod pdf;

/// 按小写扩展名把字节解析为纯文本。md/txt/空扩展按 UTF-8 文本处理。
pub fn parse_bytes(ext: &str, bytes: &[u8]) -> Result<String, String> {
    match ext.to_ascii_lowercase().as_str() {
        "pdf" => pdf::extract(bytes),
        "docx" => office::extract_docx(bytes),
        "pptx" => office::extract_pptx(bytes),
        "xlsx" => office::extract_xlsx(bytes),
        "md" | "markdown" | "txt" | "" => {
            String::from_utf8(bytes.to_vec()).map_err(|_| "文件不是有效的 UTF-8 文本".to_string())
        }
        other => Err(format!("暂不支持的文件类型：.{other}")),
    }
}

/// 从路径取小写扩展名（无则空串）。
pub fn ext_of(path: &str) -> String {
    path.rsplit('.')
        .next()
        .filter(|e| !e.contains(['/', '\\']))
        .unwrap_or("")
        .to_ascii_lowercase()
}
