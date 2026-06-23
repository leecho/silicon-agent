use super::{
    collect_local_elements, collect_local_segments, collect_tag_text, escape_html, open_zip,
    slide_number, xml_attr, zip_entry_to_string,
};

const DEFAULT_SLIDE_CX: usize = 12_192_000;
const DEFAULT_SLIDE_CY: usize = 6_858_000;

#[derive(Debug, Clone, Copy)]
struct SlideSize {
    cx: usize,
    cy: usize,
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    x: usize,
    y: usize,
    cx: usize,
    cy: usize,
}

#[derive(Debug, Clone)]
struct TextStyle {
    font_size_pt: Option<f64>,
    color: Option<String>,
    bold: bool,
}

/// 把 PPTX 转成静态幻灯片画布预览。
///
/// 只在本地解析 Open XML，不调用 Office/LibreOffice，也不执行脚本；动画、复杂图表和母版继承先降级。
pub(super) fn render_pptx_preview(bytes: &[u8]) -> Result<String, String> {
    let size = read_slide_size(bytes);
    let archive = open_zip(bytes)?;
    let mut slide_names = archive
        .file_names()
        .filter(|name| {
            name.starts_with("ppt/slides/slide")
                && name.ends_with(".xml")
                && !name.contains("_rels/")
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    slide_names.sort_by_key(|name| slide_number(name));
    slide_names.truncate(40);
    if slide_names.is_empty() {
        return Err("未找到可预览的 PowerPoint 幻灯片。".to_string());
    }

    let mut slides = Vec::new();
    for (index, slide_name) in slide_names.iter().enumerate() {
        let xml = zip_entry_to_string(bytes, slide_name)?;
        slides.push(render_slide(&xml, size, index + 1));
    }
    Ok(format!(
        r#"<div class="ppt-deck">{}</div>"#,
        slides.join("\n")
    ))
}

fn read_slide_size(bytes: &[u8]) -> SlideSize {
    let Ok(xml) = zip_entry_to_string(bytes, "ppt/presentation.xml") else {
        return default_slide_size();
    };
    collect_local_elements(&xml, "sldSz")
        .into_iter()
        .next()
        .and_then(|element| {
            Some(SlideSize {
                cx: xml_attr(element, "cx")?.parse().ok()?,
                cy: xml_attr(element, "cy")?.parse().ok()?,
            })
        })
        .filter(|size| size.cx > 0 && size.cy > 0)
        .unwrap_or_else(default_slide_size)
}

fn default_slide_size() -> SlideSize {
    SlideSize {
        cx: DEFAULT_SLIDE_CX,
        cy: DEFAULT_SLIDE_CY,
    }
}

fn render_slide(xml: &str, size: SlideSize, index: usize) -> String {
    let background = slide_background(xml).unwrap_or_else(|| "#FFFFFF".to_string());
    let mut html = format!(
        r#"<section class="ppt-slide" style="aspect-ratio: {} / {}; background: {};">"#,
        size.cx, size.cy, background
    );
    for shape in collect_local_elements(xml, "sp") {
        if let Some(layer) = render_shape(shape, size) {
            html.push_str(&layer);
        }
    }
    for frame in collect_local_elements(xml, "graphicFrame") {
        if let Some(table) = render_table(frame, size) {
            html.push_str(&table);
        }
    }
    html.push_str(&format!(
        r#"<div class="ppt-slide-label">{}</div></section>"#,
        index
    ));
    html
}

fn slide_background(xml: &str) -> Option<String> {
    let bg = collect_local_elements(xml, "bg").into_iter().next()?;
    first_srgb_color(bg)
}

fn render_shape(shape_xml: &str, size: SlideSize) -> Option<String> {
    let bounds = shape_bounds(shape_xml)?;
    let text = shape_text_html(shape_xml);
    let fill = shape_fill(shape_xml);
    if text.is_empty() && fill.is_none() {
        return None;
    }
    let style = shape_text_style(shape_xml);
    let mut css = bounds_css(bounds, size);
    if let Some(fill) = fill {
        css.push_str(&format!(" background: {};", fill));
    }
    if let Some(font_size) = style.font_size_pt {
        css.push_str(&format!(" font-size: {:.2}pt;", font_size));
    }
    if style.bold {
        css.push_str(" font-weight: 700;");
    }
    if let Some(color) = style.color {
        css.push_str(&format!(" color: {};", color));
    }
    Some(format!(
        r#"<div class="ppt-shape ppt-text" style="{}">{}</div>"#,
        css, text
    ))
}

fn shape_bounds(shape_xml: &str) -> Option<Bounds> {
    let xfrm = collect_local_elements(shape_xml, "xfrm")
        .into_iter()
        .next()?;
    let off = collect_local_elements(xfrm, "off").into_iter().next()?;
    let ext = collect_local_elements(xfrm, "ext").into_iter().next()?;
    Some(Bounds {
        x: xml_attr(off, "x")?.parse().ok()?,
        y: xml_attr(off, "y")?.parse().ok()?,
        cx: xml_attr(ext, "cx")?.parse().ok()?,
        cy: xml_attr(ext, "cy")?.parse().ok()?,
    })
}

fn render_table(frame_xml: &str, size: SlideSize) -> Option<String> {
    let bounds = shape_bounds(frame_xml)?;
    let table_xml = collect_local_elements(frame_xml, "tbl")
        .into_iter()
        .next()?;
    let rows = collect_local_segments(table_xml, "tr")
        .into_iter()
        .map(|row| {
            collect_local_segments(row, "tc")
                .into_iter()
                .map(|cell| {
                    let text = collect_tag_text(cell, &["t", "tab", "br"]);
                    escape_html(text.trim())
                })
                .collect::<Vec<_>>()
        })
        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }
    let mut html = format!(
        r#"<table class="ppt-table" style="{}"><tbody>"#,
        bounds_css(bounds, size)
    );
    for row in rows.into_iter().take(24) {
        html.push_str("<tr>");
        for cell in row.into_iter().take(12) {
            html.push_str("<td>");
            html.push_str(&cell);
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    Some(html)
}

fn bounds_css(bounds: Bounds, size: SlideSize) -> String {
    format!(
        "left: {:.2}%; top: {:.2}%; width: {:.2}%; height: {:.2}%;",
        percent(bounds.x, size.cx),
        percent(bounds.y, size.cy),
        percent(bounds.cx, size.cx),
        percent(bounds.cy, size.cy)
    )
}

fn percent(value: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        value as f64 * 100.0 / total as f64
    }
}

fn shape_text_html(shape_xml: &str) -> String {
    collect_local_segments(shape_xml, "p")
        .into_iter()
        .filter_map(|paragraph| {
            let runs = collect_local_elements(paragraph, "r")
                .into_iter()
                .filter_map(|run| {
                    let text = collect_tag_text(run, &["t", "tab", "br"]);
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                })
                .collect::<Vec<_>>();
            let text = if runs.is_empty() {
                collect_tag_text(paragraph, &["t", "tab", "br"])
            } else {
                runs.join("")
            };
            if text.trim().is_empty() {
                None
            } else {
                Some(escape_html(text.trim()))
            }
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

fn shape_text_style(shape_xml: &str) -> TextStyle {
    let rpr = collect_local_elements(shape_xml, "rPr").into_iter().next();
    TextStyle {
        font_size_pt: rpr
            .and_then(|element| xml_attr(element, "sz"))
            .and_then(|value| value.parse::<f64>().ok())
            .map(|size| size / 100.0),
        color: rpr.and_then(first_srgb_color),
        bold: rpr
            .and_then(|element| xml_attr(element, "b"))
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
    }
}

fn shape_fill(shape_xml: &str) -> Option<String> {
    let sppr = collect_local_elements(shape_xml, "spPr")
        .into_iter()
        .next()?;
    first_srgb_color(sppr)
}

fn first_srgb_color(xml: &str) -> Option<String> {
    let element = collect_local_elements(xml, "srgbClr").into_iter().next()?;
    let value = xml_attr(element, "val")?;
    if value.len() == 6 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!("#{}", value.to_ascii_uppercase()))
    } else {
        None
    }
}
