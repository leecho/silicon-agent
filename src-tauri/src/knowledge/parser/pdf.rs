//! PDF 文本抽取：薄封装 pdf-extract。扫描件/加密件可能抽不到文本，返回 Err 由上层置 error。

pub fn extract(bytes: &[u8]) -> Result<String, String> {
    match pdf_extract::extract_text_from_mem(bytes) {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        Ok(_) => Err("PDF 未能提取到文本（可能是扫描件或无文本层）".to_string()),
        Err(e) => Err(format!("PDF 解析失败：{e}")),
    }
}
