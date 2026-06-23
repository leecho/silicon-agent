use std::collections::BTreeMap;
use std::io::{Cursor, Read};

mod pptx;

/// 产物预览的 Office 文件扩展名集合。
pub fn is_office_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    matches!(
        extension(&lower).as_deref(),
        Some("docx" | "doc" | "xlsx" | "xls" | "pptx" | "ppt")
    )
}

/// 把 Word、Excel、PowerPoint 转成内存中的低保真安全 HTML 预览。
///
/// 现代 Office 文件优先读取压缩包中的 XML 文本；老旧二进制格式无法可靠解析，
/// 因此仅抽取可见文本片段作为降级预览。
pub fn render_office_preview(path: &str, bytes: &[u8]) -> String {
    let lower = path.to_lowercase();
    let title = file_name(path);
    let body = match extension(&lower).as_deref() {
        Some("docx") => render_docx(bytes),
        Some("xlsx") => render_xlsx(bytes),
        Some("pptx") => pptx::render_pptx_preview(bytes),
        Some("doc" | "xls" | "ppt") => render_legacy_office(bytes),
        _ => Ok(String::new()),
    }
    .unwrap_or_else(|message| {
        format!(
            r#"<section class="notice"><h2>无法解析内容</h2><p>{}</p></section>"#,
            escape_html(&message)
        )
    });

    format!(
        r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    :root {{
      color-scheme: light dark;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: Canvas;
      color: CanvasText;
    }}
    body {{
      margin: 0;
      padding: 28px;
      line-height: 1.6;
      overflow-wrap: anywhere;
    }}
    main {{
      max-width: 960px;
      margin: 0 auto;
    }}
    header {{
      margin-bottom: 20px;
      border-bottom: 1px solid color-mix(in srgb, CanvasText 14%, transparent);
      padding-bottom: 12px;
    }}
    h1 {{
      margin: 0;
      font-size: 22px;
      line-height: 1.3;
    }}
    h2 {{
      margin: 22px 0 10px;
      font-size: 16px;
    }}
    p {{
      margin: 0 0 12px;
      white-space: pre-wrap;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      margin: 12px 0 22px;
      font-size: 13px;
    }}
    td, th {{
      border: 1px solid color-mix(in srgb, CanvasText 14%, transparent);
      padding: 6px 8px;
      vertical-align: top;
    }}
    .muted {{
      color: color-mix(in srgb, CanvasText 62%, transparent);
      font-size: 13px;
    }}
    .notice {{
      border: 1px solid color-mix(in srgb, CanvasText 14%, transparent);
      border-radius: 8px;
      padding: 18px;
      background: color-mix(in srgb, CanvasText 5%, transparent);
    }}
    .slide {{
      border: 1px solid color-mix(in srgb, CanvasText 14%, transparent);
      border-radius: 8px;
      margin: 0 0 16px;
      padding: 16px;
    }}
    .xlsx-tabset {{
      min-width: 0;
      --xlsx-tab-text: #1f2937;
      --xlsx-tab-muted: #4b5563;
      --xlsx-tab-bg: #f8fafc;
      --xlsx-tab-hover-bg: #eef2f7;
      --xlsx-tab-active-bg: #ffffff;
      --xlsx-tab-border: #d1d5db;
      --xlsx-tab-active-border: #9ca3af;
      --xlsx-tab-accent: #2563eb;
    }}
    @media (prefers-color-scheme: dark) {{
      .xlsx-tabset {{
        --xlsx-tab-text: #f3f4f6;
        --xlsx-tab-muted: #d1d5db;
        --xlsx-tab-bg: #111827;
        --xlsx-tab-hover-bg: #1f2937;
        --xlsx-tab-active-bg: #0b1220;
        --xlsx-tab-border: #374151;
        --xlsx-tab-active-border: #6b7280;
        --xlsx-tab-accent: #60a5fa;
      }}
    }}
    .xlsx-tab-input {{
      position: absolute;
      opacity: 0;
      pointer-events: none;
    }}
    .xlsx-tab-list {{
      display: flex;
      gap: 6px;
      margin: 8px 0 14px;
      padding: 6px 6px 0;
      overflow-x: auto;
      border-bottom: 1px solid var(--xlsx-tab-border);
    }}
    .xlsx-tab {{
      display: inline-flex;
      min-height: 34px;
      align-items: center;
      border: 1px solid var(--xlsx-tab-border);
      border-bottom: 0;
      border-radius: 8px 8px 0 0;
      padding: 0 14px;
      background: var(--xlsx-tab-bg);
      color: var(--xlsx-tab-muted);
      cursor: pointer;
      font-weight: 500;
      white-space: nowrap;
      user-select: none;
    }}
    .xlsx-tab:hover {{
      background: var(--xlsx-tab-hover-bg);
      color: var(--xlsx-tab-text);
    }}
    .xlsx-panels {{
      min-width: 0;
    }}
    .xlsx-sheet-panel {{
      display: none;
      min-width: 0;
      overflow-x: auto;
    }}
    #xlsx-sheet-0:checked ~ .xlsx-tab-list label[for="xlsx-sheet-0"],
    #xlsx-sheet-1:checked ~ .xlsx-tab-list label[for="xlsx-sheet-1"],
    #xlsx-sheet-2:checked ~ .xlsx-tab-list label[for="xlsx-sheet-2"],
    #xlsx-sheet-3:checked ~ .xlsx-tab-list label[for="xlsx-sheet-3"],
    #xlsx-sheet-4:checked ~ .xlsx-tab-list label[for="xlsx-sheet-4"] {{
      border-color: var(--xlsx-tab-active-border);
      background: var(--xlsx-tab-active-bg);
      color: var(--xlsx-tab-text);
      font-weight: 600;
      box-shadow: inset 0 2px 0 var(--xlsx-tab-accent);
    }}
    #xlsx-sheet-0:checked ~ .xlsx-panels [data-sheet-index="0"],
    #xlsx-sheet-1:checked ~ .xlsx-panels [data-sheet-index="1"],
    #xlsx-sheet-2:checked ~ .xlsx-panels [data-sheet-index="2"],
    #xlsx-sheet-3:checked ~ .xlsx-panels [data-sheet-index="3"],
    #xlsx-sheet-4:checked ~ .xlsx-panels [data-sheet-index="4"] {{
      display: block;
    }}
    .ppt-deck {{
      display: grid;
      gap: 22px;
    }}
    .ppt-slide {{
      position: relative;
      width: 100%;
      overflow: hidden;
      border: 1px solid color-mix(in srgb, CanvasText 16%, transparent);
      border-radius: 8px;
      box-shadow: 0 10px 30px color-mix(in srgb, CanvasText 10%, transparent);
    }}
    .ppt-shape {{
      position: absolute;
      box-sizing: border-box;
      overflow: hidden;
      white-space: pre-wrap;
      overflow-wrap: break-word;
    }}
    .ppt-text {{
      display: flex;
      flex-direction: column;
      justify-content: flex-start;
      line-height: 1.25;
    }}
    .ppt-table {{
      position: absolute;
      box-sizing: border-box;
      border-collapse: collapse;
      table-layout: fixed;
      overflow: hidden;
      font-size: 11pt;
      background: color-mix(in srgb, Canvas 92%, transparent);
    }}
    .ppt-table td {{
      border: 1px solid color-mix(in srgb, CanvasText 18%, transparent);
      padding: 4px 6px;
      vertical-align: middle;
      overflow-wrap: anywhere;
    }}
    .ppt-slide-label {{
      position: absolute;
      right: 10px;
      bottom: 8px;
      color: color-mix(in srgb, CanvasText 52%, transparent);
      font-size: 11px;
      pointer-events: none;
    }}
  </style>
</head>
<body>
  <main>
    <header>
      <h1>{}</h1>
      <div class="muted">Office 预览 · 内容结构视图</div>
    </header>
    {}
  </main>
</body>
</html>"#,
        escape_html(title),
        body
    )
}

fn extension(path: &str) -> Option<String> {
    path.rsplit('.')
        .next()
        .map(str::to_string)
        .filter(|ext| ext != path)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

fn render_docx(bytes: &[u8]) -> Result<String, String> {
    let xml = zip_entry_to_string(bytes, "word/document.xml")?;
    let body = first_local_segment(&xml, "body").unwrap_or(xml.as_str());
    let blocks = render_docx_blocks(body);
    if blocks.is_empty() {
        return Err("未找到可预览的 Word 文本。".to_string());
    }
    Ok(blocks.join("\n"))
}

fn render_docx_blocks(body_xml: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut offset = 0;
    while let Some(open_rel) = body_xml[offset..].find('<') {
        let open = offset + open_rel;
        let Some(tag) = parse_xml_tag(body_xml, open) else {
            break;
        };
        if tag.closing {
            offset = tag.end;
            continue;
        }
        match tag.local_name {
            "p" => {
                if let Some(segment) = matching_local_segment(body_xml, open, "p") {
                    if let Some(paragraph) = render_docx_paragraph(segment.content) {
                        blocks.push(paragraph);
                    }
                    offset = segment.after_close;
                } else {
                    offset = tag.end;
                }
            }
            "tbl" => {
                if let Some(segment) = matching_local_segment(body_xml, open, "tbl") {
                    if let Some(table) = render_docx_table(segment.content) {
                        blocks.push(table);
                    }
                    offset = segment.after_close;
                } else {
                    offset = tag.end;
                }
            }
            _ => {
                offset = tag.end;
            }
        }
    }
    blocks.into_iter().take(240).collect()
}

fn render_docx_paragraph(paragraph_xml: &str) -> Option<String> {
    let text = normalize_docx_text(&collect_tag_text(paragraph_xml, &["t", "tab", "br"]));
    if text.is_empty() {
        None
    } else {
        Some(format!("<p>{}</p>", escape_html(&text)))
    }
}

fn render_docx_table(table_xml: &str) -> Option<String> {
    let mut html = String::from("<table><tbody>");
    let mut has_cells = false;
    for row in collect_local_segments(table_xml, "tr").into_iter().take(80) {
        let cells = collect_local_segments(row, "tc");
        if cells.is_empty() {
            continue;
        }
        html.push_str("<tr>");
        for cell in cells.into_iter().take(20) {
            let text = render_docx_cell_text(cell);
            has_cells = has_cells || !text.is_empty();
            html.push_str("<td>");
            html.push_str(&text);
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    if has_cells {
        Some(html)
    } else {
        None
    }
}

fn render_docx_cell_text(cell_xml: &str) -> String {
    let paragraphs = collect_local_segments(cell_xml, "p")
        .into_iter()
        .filter_map(|paragraph| {
            let text = normalize_docx_text(&collect_tag_text(paragraph, &["t", "tab", "br"]));
            if text.is_empty() {
                None
            } else {
                Some(escape_html(&text))
            }
        })
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        escape_html(&normalize_docx_text(&collect_tag_text(
            cell_xml,
            &["t", "tab", "br"],
        )))
    } else {
        paragraphs.join("<br>")
    }
}

fn render_xlsx(bytes: &[u8]) -> Result<String, String> {
    let shared_strings = read_shared_strings(bytes);
    let archive = open_zip(bytes)?;
    let mut sheet_names = archive
        .file_names()
        .filter(|name| {
            name.starts_with("xl/worksheets/sheet")
                && name.ends_with(".xml")
                && !name.contains("_rels/")
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    sheet_names.sort();
    sheet_names.truncate(5);
    if sheet_names.is_empty() {
        return Err("未找到可预览的 Excel 工作表。".to_string());
    }

    let mut sections = Vec::new();
    for (index, sheet_name) in sheet_names.iter().enumerate() {
        let xml = zip_entry_to_string(bytes, sheet_name)?;
        let rows = extract_sheet_rows(&xml, &shared_strings);
        if rows.is_empty() {
            continue;
        }
        let mut table = String::from("<table><tbody>");
        for row in rows.into_iter().take(80) {
            table.push_str("<tr>");
            for cell in row {
                table.push_str("<td");
                if cell.colspan > 1 {
                    table.push_str(&format!(r#" colspan="{}""#, cell.colspan));
                }
                if cell.rowspan > 1 {
                    table.push_str(&format!(r#" rowspan="{}""#, cell.rowspan));
                }
                table.push('>');
                table.push_str(&escape_html(&cell.value));
                table.push_str("</td>");
            }
            table.push_str("</tr>");
        }
        table.push_str("</tbody></table>");
        sections.push((format!("Sheet {}", index + 1), table));
    }
    if sections.is_empty() {
        Err("未找到可预览的 Excel 单元格内容。".to_string())
    } else {
        Ok(render_xlsx_tabs(sections))
    }
}

fn render_xlsx_tabs(sheets: Vec<(String, String)>) -> String {
    if sheets.len() <= 1 {
        return sheets
            .into_iter()
            .map(|(label, table)| format!("<h2>{}</h2>{}", escape_html(&label), table))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let mut html = String::from(r#"<div class="xlsx-tabset">"#);
    for index in 0..sheets.len() {
        html.push_str(&format!(
            r#"<input class="xlsx-tab-input" type="radio" name="xlsx-tabs" id="xlsx-sheet-{}"{}>"#,
            index,
            if index == 0 { " checked" } else { "" }
        ));
    }
    html.push_str(r#"<div class="xlsx-tab-list" role="tablist">"#);
    for (index, (label, _)) in sheets.iter().enumerate() {
        html.push_str(&format!(
            r#"<label class="xlsx-tab" for="xlsx-sheet-{}">{}</label>"#,
            index,
            escape_html(label)
        ));
    }
    html.push_str("</div><div class=\"xlsx-panels\">");
    for (index, (_, table)) in sheets.into_iter().enumerate() {
        html.push_str(&format!(
            r#"<section class="xlsx-sheet-panel" data-sheet-index="{}">{} </section>"#,
            index, table
        ));
    }
    html.push_str("</div></div>");
    html
}

fn render_legacy_office(bytes: &[u8]) -> Result<String, String> {
    let text = extract_legacy_text(bytes);
    let paragraphs = text_to_paragraphs(&text);
    if paragraphs.is_empty() {
        return Err(
            "老旧 Office 二进制格式只能做文本降级预览，当前文件未提取到可见文本。".to_string(),
        );
    }
    let mut body = String::from(
        r#"<section class="notice"><p>老旧 Office 二进制格式采用文本降级预览，排版、表格和图片不会还原。</p></section>"#,
    );
    for paragraph in paragraphs.into_iter().take(120) {
        body.push_str("<p>");
        body.push_str(&escape_html(&paragraph));
        body.push_str("</p>");
    }
    Ok(body)
}

fn open_zip(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, String> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("读取 Office 压缩包失败：{e}"))
}

fn zip_entry_to_string(bytes: &[u8], name: &str) -> Result<String, String> {
    let mut archive = open_zip(bytes)?;
    let mut file = archive
        .by_name(name)
        .map_err(|e| format!("读取 Office 条目 {name} 失败：{e}"))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("读取 Office XML 失败：{e}"))?;
    Ok(content)
}

fn read_shared_strings(bytes: &[u8]) -> Vec<String> {
    let Ok(xml) = zip_entry_to_string(bytes, "xl/sharedStrings.xml") else {
        return Vec::new();
    };
    collect_local_segments(&xml, "si")
        .into_iter()
        .map(|segment| collect_tag_text(segment, &["t", "tab", "br"]))
        .collect()
}

struct XlsxPreviewCell {
    value: String,
    colspan: usize,
    rowspan: usize,
}

#[derive(Debug, Clone)]
struct XlsxMergeRange {
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
}

fn extract_sheet_rows(xml: &str, shared_strings: &[String]) -> Vec<Vec<XlsxPreviewCell>> {
    let merges = extract_merge_ranges(xml);
    let mut source_rows = Vec::new();
    let mut max_col = 0usize;
    for (row_order, row_xml) in collect_local_elements(xml, "row").into_iter().enumerate() {
        let row_index = xml_attr(row_xml, "r")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(row_order + 1);
        let mut cells = BTreeMap::new();
        let mut next_col = 1usize;
        for cell_xml in collect_local_elements(row_xml, "c") {
            let col = xml_attr(cell_xml, "r")
                .and_then(|reference| parse_cell_ref(&reference).map(|(_, col)| col))
                .unwrap_or(next_col);
            next_col = col + 1;
            max_col = max_col.max(col);
            cells.insert(col, extract_cell_value(cell_xml, shared_strings));
        }
        source_rows.push((row_index, cells));
    }
    for merge in &merges {
        max_col = max_col.max(merge.end_col);
    }
    let max_col = max_col.min(20);
    source_rows
        .into_iter()
        .filter_map(|(row_index, cells)| render_sheet_row(row_index, &cells, &merges, max_col))
        .collect()
}

fn render_sheet_row(
    row_index: usize,
    cells: &BTreeMap<usize, String>,
    merges: &[XlsxMergeRange],
    max_col: usize,
) -> Option<Vec<XlsxPreviewCell>> {
    let mut output = Vec::new();
    let mut col = 1usize;
    while col <= max_col {
        if merge_covering_non_start(merges, row_index, col) {
            col += 1;
            continue;
        }
        let merge = merge_starting_at(merges, row_index, col);
        let colspan = merge
            .map(|range| range.end_col.min(max_col).saturating_sub(col) + 1)
            .unwrap_or(1);
        let rowspan = merge
            .map(|range| range.end_row.saturating_sub(row_index) + 1)
            .unwrap_or(1);
        output.push(XlsxPreviewCell {
            value: cells.get(&col).cloned().unwrap_or_default(),
            colspan,
            rowspan,
        });
        col += colspan.max(1);
    }
    if output.iter().any(|cell| !cell.value.trim().is_empty()) {
        Some(output)
    } else {
        None
    }
}

fn extract_merge_ranges(xml: &str) -> Vec<XlsxMergeRange> {
    collect_local_elements(xml, "mergeCell")
        .into_iter()
        .filter_map(|element| xml_attr(element, "ref"))
        .filter_map(|reference| parse_cell_range(&reference))
        .collect()
}

fn merge_starting_at(merges: &[XlsxMergeRange], row: usize, col: usize) -> Option<&XlsxMergeRange> {
    merges
        .iter()
        .find(|range| range.start_row == row && range.start_col == col)
}

fn merge_covering_non_start(merges: &[XlsxMergeRange], row: usize, col: usize) -> bool {
    merges.iter().any(|range| {
        row >= range.start_row
            && row <= range.end_row
            && col >= range.start_col
            && col <= range.end_col
            && !(row == range.start_row && col == range.start_col)
    })
}

fn extract_cell_value(cell_xml: &str, shared_strings: &[String]) -> String {
    let raw = first_tag_text(cell_xml, "v")
        .or_else(|| first_tag_text(cell_xml, "t"))
        .unwrap_or_default();
    if cell_xml.contains(r#"t="s""#) || cell_xml.contains("t='s'") {
        return raw
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|idx| shared_strings.get(idx).cloned())
            .unwrap_or(raw);
    }
    decode_xml_entities(raw.trim())
}

fn parse_cell_range(reference: &str) -> Option<XlsxMergeRange> {
    let (start, end) = reference.split_once(':').unwrap_or((reference, reference));
    let (start_row, start_col) = parse_cell_ref(start)?;
    let (end_row, end_col) = parse_cell_ref(end)?;
    Some(XlsxMergeRange {
        start_row: start_row.min(end_row),
        start_col: start_col.min(end_col),
        end_row: start_row.max(end_row),
        end_col: start_col.max(end_col),
    })
}

fn parse_cell_ref(reference: &str) -> Option<(usize, usize)> {
    let mut col = 0usize;
    let mut row = String::new();
    for ch in reference.chars() {
        if ch.is_ascii_alphabetic() {
            col = col * 26 + (ch.to_ascii_uppercase() as u8 - b'A' + 1) as usize;
        } else if ch.is_ascii_digit() {
            row.push(ch);
        } else if ch == '$' {
            continue;
        } else {
            return None;
        }
    }
    let row = row.parse::<usize>().ok()?;
    if row == 0 || col == 0 {
        None
    } else {
        Some((row, col))
    }
}

fn xml_attr(element_xml: &str, attr: &str) -> Option<String> {
    let open = element_xml.find('<')?;
    let close = element_xml[open..].find('>')? + open;
    let tag = &element_xml[open + 1..close];
    let bytes = tag.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        let name_start = offset;
        while offset < bytes.len()
            && !bytes[offset].is_ascii_whitespace()
            && bytes[offset] != b'='
            && bytes[offset] != b'/'
        {
            offset += 1;
        }
        let name = &tag[name_start..offset];
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        if offset >= bytes.len() || bytes[offset] != b'=' {
            continue;
        }
        offset += 1;
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        if offset >= bytes.len() || !matches!(bytes[offset], b'\'' | b'"') {
            continue;
        }
        let quote = bytes[offset];
        offset += 1;
        let value_start = offset;
        while offset < bytes.len() && bytes[offset] != quote {
            offset += 1;
        }
        let value = &tag[value_start..offset];
        if offset < bytes.len() {
            offset += 1;
        }
        let local_name = name.rsplit(':').next().unwrap_or(name);
        if local_name == attr {
            return Some(decode_xml_entities(value));
        }
    }
    None
}

struct XmlTag<'a> {
    local_name: &'a str,
    closing: bool,
    self_closing: bool,
    end: usize,
}

struct XmlSegment<'a> {
    content: &'a str,
    after_close: usize,
}

fn first_local_segment<'a>(xml: &'a str, local_name: &str) -> Option<&'a str> {
    let mut offset = 0;
    while let Some(open_rel) = xml[offset..].find('<') {
        let open = offset + open_rel;
        let tag = parse_xml_tag(xml, open)?;
        if !tag.closing && tag.local_name == local_name {
            return matching_local_segment(xml, open, local_name).map(|segment| segment.content);
        }
        offset = tag.end;
    }
    None
}

fn collect_local_segments<'a>(xml: &'a str, local_name: &str) -> Vec<&'a str> {
    let mut segments = Vec::new();
    let mut offset = 0;
    while let Some(open_rel) = xml[offset..].find('<') {
        let open = offset + open_rel;
        let Some(tag) = parse_xml_tag(xml, open) else {
            break;
        };
        if !tag.closing && tag.local_name == local_name {
            if let Some(segment) = matching_local_segment(xml, open, local_name) {
                segments.push(segment.content);
                offset = segment.after_close;
                continue;
            }
        }
        offset = tag.end;
    }
    segments
}

fn collect_local_elements<'a>(xml: &'a str, local_name: &str) -> Vec<&'a str> {
    let mut elements = Vec::new();
    let mut offset = 0;
    while let Some(open_rel) = xml[offset..].find('<') {
        let open = offset + open_rel;
        let Some(tag) = parse_xml_tag(xml, open) else {
            break;
        };
        if !tag.closing && tag.local_name == local_name {
            if let Some(segment) = matching_local_segment(xml, open, local_name) {
                elements.push(&xml[open..segment.after_close]);
                offset = segment.after_close;
                continue;
            }
        }
        offset = tag.end;
    }
    elements
}

fn matching_local_segment<'a>(
    xml: &'a str,
    open: usize,
    local_name: &str,
) -> Option<XmlSegment<'a>> {
    let start_tag = parse_xml_tag(xml, open)?;
    if start_tag.closing || start_tag.local_name != local_name {
        return None;
    }
    if start_tag.self_closing {
        return Some(XmlSegment {
            content: "",
            after_close: start_tag.end,
        });
    }
    let content_start = start_tag.end;
    let mut depth = 1usize;
    let mut offset = start_tag.end;
    while let Some(next_rel) = xml[offset..].find('<') {
        let next = offset + next_rel;
        let Some(tag) = parse_xml_tag(xml, next) else {
            break;
        };
        if tag.local_name == local_name {
            if tag.closing {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(XmlSegment {
                        content: &xml[content_start..next],
                        after_close: tag.end,
                    });
                }
            } else if !tag.self_closing {
                depth += 1;
            }
        }
        offset = tag.end;
    }
    None
}

fn parse_xml_tag(xml: &str, open: usize) -> Option<XmlTag<'_>> {
    if !xml[open..].starts_with('<') {
        return None;
    }
    let close = open + xml[open..].find('>')?;
    let raw = xml[open + 1..close].trim();
    if raw.starts_with('!') || raw.starts_with('?') {
        return Some(XmlTag {
            local_name: "",
            closing: false,
            self_closing: true,
            end: close + 1,
        });
    }
    let closing = raw.starts_with('/');
    let raw_name = raw
        .trim_start_matches('/')
        .trim_end_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("");
    let local_name = raw_name.rsplit(':').next().unwrap_or(raw_name);
    Some(XmlTag {
        local_name,
        closing,
        self_closing: raw.ends_with('/'),
        end: close + 1,
    })
}

fn collect_tag_text(xml: &str, text_tags: &[&str]) -> String {
    let mut output = String::new();
    let mut offset = 0;
    while let Some(open_rel) = xml[offset..].find('<') {
        let open = offset + open_rel;
        let text = &xml[offset..open];
        if !text.trim().is_empty() {
            output.push_str(&decode_xml_entities(text));
        }
        let Some(close_rel) = xml[open..].find('>') else {
            break;
        };
        let close = open + close_rel;
        let tag_content = xml[open + 1..close].trim();
        let tag_name = tag_content
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .rsplit(':')
            .next()
            .unwrap_or("");
        if text_tags.contains(&tag_name) {
            if matches!(tag_name, "br" | "tab") {
                output.push(if tag_name == "tab" { '\t' } else { '\n' });
            }
        } else if matches!(tag_name, "p" | "row" | "tr") {
            output.push('\n');
        }
        offset = close + 1;
    }
    if offset < xml.len() {
        output.push_str(&decode_xml_entities(&xml[offset..]));
    }
    output
}

fn normalize_docx_text(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_tag_text(xml: &str, tag: &str) -> Option<String> {
    collect_local_segments(xml, tag)
        .into_iter()
        .next()
        .map(|segment| decode_xml_entities(segment.trim()))
}

fn text_to_paragraphs(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .take(240)
        .collect()
}

fn extract_legacy_text(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut current = String::new();
    for &byte in bytes {
        let ch = byte as char;
        if ch.is_ascii_graphic() || ch == ' ' || ch == '\n' || ch == '\t' {
            current.push(ch);
        } else if byte == 0 {
            continue;
        } else {
            flush_legacy_token(&mut current, &mut output);
        }
    }
    flush_legacy_token(&mut current, &mut output);
    output
}

fn flush_legacy_token(current: &mut String, output: &mut String) {
    let token = current.trim();
    if token.chars().count() >= 2 {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(token);
    }
    current.clear();
}

fn slide_number(name: &str) -> usize {
    let file = file_name(name);
    file.trim_start_matches("slide")
        .trim_end_matches(".xml")
        .parse()
        .unwrap_or(usize::MAX)
}

fn decode_xml_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut offset = 0;
    while let Some(amp_rel) = input[offset..].find('&') {
        let amp = offset + amp_rel;
        output.push_str(&input[offset..amp]);
        let entity_start = amp + 1;
        let Some(semi_rel) = input[entity_start..].find(';') else {
            output.push_str(&input[amp..]);
            return output;
        };
        let semi = entity_start + semi_rel;
        let entity = &input[entity_start..semi];
        if let Some(decoded) = decode_xml_entity(entity) {
            output.push_str(&decoded);
        } else {
            output.push('&');
            output.push_str(entity);
            output.push(';');
        }
        offset = semi + 1;
    }
    output.push_str(&input[offset..]);
    output
}

fn decode_xml_entity(entity: &str) -> Option<String> {
    let decoded = match entity {
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "amp" => "&".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            let value = u32::from_str_radix(&entity[2..], 16).ok()?;
            char::from_u32(value)?.to_string()
        }
        _ if entity.starts_with('#') => {
            let value = entity[1..].parse::<u32>().ok()?;
            char::from_u32(value)?.to_string()
        }
        _ => return None,
    };
    Some(decoded)
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::render_office_preview;
    use std::io::{Cursor, Write};

    fn minimal_docx(document_xml: &str) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file(
                    "word/document.xml",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(document_xml.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn minimal_xlsx(shared_strings_xml: &str, sheet_xml: &str) -> Vec<u8> {
        minimal_xlsx_sheets(shared_strings_xml, &[sheet_xml])
    }

    fn minimal_xlsx_sheets(shared_strings_xml: &str, sheets: &[&str]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("xl/sharedStrings.xml", options).unwrap();
            writer.write_all(shared_strings_xml.as_bytes()).unwrap();
            for (index, sheet_xml) in sheets.iter().enumerate() {
                writer
                    .start_file(format!("xl/worksheets/sheet{}.xml", index + 1), options)
                    .unwrap();
                writer.write_all(sheet_xml.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn minimal_pptx(presentation_xml: &str, slide_xml: &str) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("ppt/presentation.xml", options).unwrap();
            writer.write_all(presentation_xml.as_bytes()).unwrap();
            writer.start_file("ppt/slides/slide1.xml", options).unwrap();
            writer.write_all(slide_xml.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn docx_preview_preserves_word_tables() {
        let docx = minimal_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>供应商比价报告</w:t></w:r></w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:p><w:r><w:t>项目</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>内容</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc><w:p><w:r><w:t>产品名称</w:t></w:r></w:p></w:tc>
        <w:tc><w:p><w:r><w:t>烯啶虫胺 10%</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
    <w:p><w:r><w:t>渠道报价汇总</w:t></w:r></w:p>
  </w:body>
</w:document>"#,
        );

        let html = render_office_preview("report.docx", &docx);

        assert!(html.contains("<table><tbody>"));
        assert!(html.contains("<td>项目</td>"));
        assert!(html.contains("<td>内容</td>"));
        assert!(html.contains("<td>产品名称</td>"));
        assert!(html.contains("<td>烯啶虫胺 10%</td>"));
        assert!(html.contains("<p>渠道报价汇总</p>"));
    }

    #[test]
    fn xlsx_preview_decodes_numeric_character_references() {
        let xlsx = minimal_xlsx(
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <si><t>&#28911;&#21878;&#34411;&#33018;</t></si>
  <si><t>Nitenpyram</t></si>
</sst>"#,
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row>
      <c t="s"><v>0</v></c>
      <c t="s"><v>1</v></c>
    </row>
  </sheetData>
</worksheet>"#,
        );

        let html = render_office_preview("report.xlsx", &xlsx);

        assert!(html.contains("<td>烯啶虫胺</td>"));
        assert!(html.contains("<td>Nitenpyram</td>"));
        assert!(!html.contains("&amp;#28911;"));
    }

    #[test]
    fn xlsx_preview_preserves_merged_cells() {
        let xlsx = minimal_xlsx(
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <si><t>渠道报价汇总</t></si>
  <si><t>供应商</t></si>
  <si><t>说明</t></si>
  <si><t>湖北威德利化学试剂</t></si>
  <si><t>同上</t></si>
</sst>"#,
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="s"><v>0</v></c>
    </row>
    <row r="2">
      <c r="A2" t="s"><v>1</v></c>
      <c r="C2" t="s"><v>2</v></c>
    </row>
    <row r="3">
      <c r="A3" t="s"><v>3</v></c>
      <c r="C3" t="s"><v>4</v></c>
    </row>
    <row r="4">
      <c r="C4" t="s"><v>4</v></c>
    </row>
  </sheetData>
  <mergeCells count="2">
    <mergeCell ref="A1:C1"/>
    <mergeCell ref="A3:A4"/>
  </mergeCells>
</worksheet>"#,
        );

        let html = render_office_preview("report.xlsx", &xlsx);

        assert!(html.contains(r#"<td colspan="3">渠道报价汇总</td>"#));
        assert!(html.contains(r#"<td rowspan="2">湖北威德利化学试剂</td>"#));
        assert!(html.contains("<td></td><td>说明</td>"));
        assert!(!html.contains(r#"<td>渠道报价汇总</td>"#));
    }

    #[test]
    fn xlsx_preview_uses_tabs_for_multiple_sheets() {
        let xlsx = minimal_xlsx_sheets(
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <si><t>原药报价</t></si>
  <si><t>采购建议</t></si>
</sst>"#,
            &[
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData><row><c t="s"><v>0</v></c></row></sheetData>
</worksheet>"#,
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData><row><c t="s"><v>1</v></c></row></sheetData>
</worksheet>"#,
            ],
        );

        let html = render_office_preview("report.xlsx", &xlsx);

        assert!(html.contains(r#"<div class="xlsx-tabset">"#));
        assert!(html.contains(r#"<input class="xlsx-tab-input" type="radio" name="xlsx-tabs" id="xlsx-sheet-0" checked>"#));
        assert!(html.contains(
            r#"<input class="xlsx-tab-input" type="radio" name="xlsx-tabs" id="xlsx-sheet-1">"#
        ));
        assert!(html.contains(r#"<label class="xlsx-tab" for="xlsx-sheet-0">Sheet 1</label>"#));
        assert!(html.contains(r#"<label class="xlsx-tab" for="xlsx-sheet-1">Sheet 2</label>"#));
        assert!(html.contains(r#"<section class="xlsx-sheet-panel" data-sheet-index="0">"#));
        assert!(html.contains(r#"<section class="xlsx-sheet-panel" data-sheet-index="1">"#));
        assert!(!html.contains("<h2>Sheet 1</h2>"));
    }

    #[test]
    fn xlsx_tabs_use_explicit_visible_theme_colors() {
        let html = render_office_preview(
            "report.xlsx",
            &minimal_xlsx_sheets(
                r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <si><t>原药报价</t></si>
  <si><t>采购建议</t></si>
</sst>"#,
                &[
                    r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData><row><c t="s"><v>0</v></c></row></sheetData>
</worksheet>"#,
                    r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData><row><c t="s"><v>1</v></c></row></sheetData>
</worksheet>"#,
                ],
            ),
        );

        assert!(html.contains("--xlsx-tab-text: #1f2937;"));
        assert!(html.contains("--xlsx-tab-active-bg: #ffffff;"));
        assert!(html.contains("color: var(--xlsx-tab-text);"));
        assert!(html.contains("background: var(--xlsx-tab-active-bg);"));
    }

    #[test]
    fn pptx_preview_renders_slide_canvas_with_positioned_text() {
        let pptx = minimal_pptx(
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldSz cx="1000" cy="500"/>
</p:presentation>"#,
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <p:bg>
      <p:bgPr><a:solidFill><a:srgbClr val="008C95"/></a:solidFill></p:bgPr>
    </p:bg>
    <p:spTree>
      <p:sp>
        <p:spPr>
          <a:xfrm>
            <a:off x="100" y="100"/>
            <a:ext cx="400" cy="80"/>
          </a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:p>
            <a:r>
              <a:rPr sz="2800" b="1"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr>
              <a:t>比价概览</a:t>
            </a:r>
          </a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#,
        );

        let html = render_office_preview("deck.pptx", &pptx);

        assert!(html.contains(r#"<section class="ppt-slide""#));
        assert!(html.contains("aspect-ratio: 1000 / 500"));
        assert!(html.contains("background: #008C95"));
        assert!(html.contains("left: 10.00%; top: 20.00%; width: 40.00%; height: 16.00%"));
        assert!(html.contains("font-size: 28.00pt"));
        assert!(html.contains("font-weight: 700"));
        assert!(html.contains("color: #FFFFFF"));
        assert!(html.contains("比价概览"));
    }

    #[test]
    fn pptx_preview_renders_positioned_tables() {
        let pptx = minimal_pptx(
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldSz cx="1000" cy="500"/>
</p:presentation>"#,
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:graphicFrame>
        <a:xfrm>
          <a:off x="100" y="100"/>
          <a:ext cx="800" cy="200"/>
        </a:xfrm>
        <a:graphic>
          <a:graphicData>
            <a:tbl>
              <a:tr>
                <a:tc><a:txBody><a:p><a:r><a:t>供应商</a:t></a:r></a:p></a:txBody></a:tc>
                <a:tc><a:txBody><a:p><a:r><a:t>价格</a:t></a:r></a:p></a:txBody></a:tc>
              </a:tr>
              <a:tr>
                <a:tc><a:txBody><a:p><a:r><a:t>湖北威德利</a:t></a:r></a:p></a:txBody></a:tc>
                <a:tc><a:txBody><a:p><a:r><a:t>800</a:t></a:r></a:p></a:txBody></a:tc>
              </a:tr>
            </a:tbl>
          </a:graphicData>
        </a:graphic>
      </p:graphicFrame>
    </p:spTree>
  </p:cSld>
</p:sld>"#,
        );

        let html = render_office_preview("deck.pptx", &pptx);

        assert!(html.contains(r#"<table class="ppt-table" style="left: 10.00%; top: 20.00%; width: 80.00%; height: 40.00%;">"#));
        assert!(html.contains("<td>供应商</td><td>价格</td>"));
        assert!(html.contains("<td>湖北威德利</td><td>800</td>"));
    }
}
