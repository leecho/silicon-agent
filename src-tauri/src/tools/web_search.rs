use std::time::Duration;

use crate::tools::Tool;

/// 搜索后端：Bing（免 key）。DuckDuckGo html/lite 端点已被反爬拦截（返回 202 挑战页或重置 TLS），
/// 改用 Bing 中文站，对中文/英文查询都能稳定返回结果块。
const SEARCH_ENDPOINT: &str = "https://cn.bing.com/search";
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 一条搜索结果（标题/真实 URL/摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// `web_search` 工具：用 Bing（免 key）搜索网络。
pub struct WebSearch;

impl WebSearch {
    pub fn new() -> Self {
        WebSearch
    }
}

impl Default for WebSearch {
    fn default() -> Self {
        Self::new()
    }
}

/// 搜索不可用时返回给模型的明确指引。区别于"未找到结果"：让模型不要反复重试搜索、
/// 也不要改用命令行抓取，而是基于已有知识作答或请用户提供资料，避免空转。
fn search_unavailable(detail: &str) -> String {
    format!(
        "搜索服务暂时不可用（{detail}）。请基于已有知识回答，或请用户提供资料；\
         不要反复重试搜索，也不要改用命令行抓取网页。"
    )
}

impl Tool for WebSearch {
    fn name(&self) -> &str {
        "web_search"
    }

    fn label(&self) -> &str {
        "搜索网页"
    }

    fn description(&self) -> &str {
        "用 Bing 搜索网络，返回标题/摘要/链接。query 为搜索词。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "搜索词"},
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "description": "最多返回几条(默认 5,范围 1..10)"
                }
            },
            "required": ["query"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        true
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("缺少 query")?;
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|v| v.clamp(1, 10) as usize)
            .unwrap_or(5);

        let url = format!("{SEARCH_ENDPOINT}?q={}", percent_encode(query));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(15))
            .build();
        let body = match agent
            .get(&url)
            .set("User-Agent", BROWSER_UA)
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .call()
        {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| format!("搜索失败: 读取响应出错: {e}"))?,
            // HTTP 4xx/5xx（含反爬 429/403 等）：搜索服务不可用，明确告知模型别空转。
            Err(ureq::Error::Status(code, _)) => {
                return Err(search_unavailable(&format!("HTTP {code}")));
            }
            // 传输层失败（TLS/连接/超时）：同样明确告知不可用。
            Err(err) => return Err(search_unavailable(&err.to_string())),
        };

        let hits = parse_bing_results(&body, max_results);
        if hits.is_empty() {
            // 200 但解析为空：极可能被反爬拦截（挑战页）或真无结果——都给同一引导，避免误判。
            return Ok(
                "未找到结果（搜索引擎可能限流或拦截）。如多次为空，请基于已有知识回答\
                       或请用户提供资料，不要反复重试搜索。"
                    .into(),
            );
        }
        Ok(format_hits(&hits))
    }
}

/// 把结果列表格式化为纯文本：`N. {title}\n   {snippet}\n   {url}\n`。
fn format_hits(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for (i, hit) in hits.iter().enumerate() {
        out.push_str(&format!(
            "{}. {}\n   {}\n   {}\n",
            i + 1,
            hit.title,
            hit.snippet,
            hit.url
        ));
    }
    out
}

/// 解析 Bing 结果页 → 结果列表（取前 `max_results` 条）。
///
/// 真实结构：标题 `<h2 ...><a ... href="真实URL">标题</a></h2>`（href 即目标站点，无 uddg 包装）；
/// 摘要为就近的 `<p class="...b_lineclamp...">摘要</p>`。只收 http(s) 链接、标题非空的条目，
/// 借此过滤导航/相关搜索等非结果 `<h2>`。鲁棒原则：尽力提取，缺字段用空串，绝不 panic。
/// 所有切分锚点（`<h2` / `<a` / `>` / `</a>` / `<p` / `</p>` / `b_lineclamp`）均为 ASCII，
/// 字节切片落在字符边界上，UTF-8 安全。
pub fn parse_bing_results(html: &str, max_results: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut cursor = 0usize;
    while hits.len() < max_results {
        // 下一个标题 <h2 ...>
        let h2 = match find_from(html, "<h2", cursor) {
            Some(p) => p,
            None => break,
        };
        // h2 开标签结束
        let h2_gt = match find_from(html, ">", h2) {
            Some(p) => p + 1,
            None => break,
        };
        // 标题里的第一个 <a ...>
        let a = match find_from(html, "<a", h2_gt) {
            Some(p) => p,
            None => {
                cursor = h2_gt;
                continue;
            }
        };
        let a_gt = match find_from(html, ">", a) {
            Some(p) => p,
            None => break,
        };
        let href = extract_href(&html[a..a_gt]).unwrap_or_default();
        let inner_start = a_gt + 1;
        let a_close = match find_from(html, "</a>", inner_start) {
            Some(p) => p,
            None => break,
        };
        let title = clean_text(&html[inner_start..a_close]);
        cursor = a_close + "</a>".len();

        // 仅收 http(s) 链接且标题非空的结果，过滤导航/相关搜索等非结果 h2。
        if !(href.starts_with("http://") || href.starts_with("https://")) || title.is_empty() {
            continue;
        }

        // 摘要：本结果之后、下一个 <h2 之前，找 b_lineclamp 段并取 <p> 内层。
        let next_h2 = find_from(html, "<h2", cursor).unwrap_or(html.len());
        let snippet = find_from(html, "b_lineclamp", cursor)
            .filter(|&p| p < next_h2)
            .and_then(|p| {
                let p_open = html[..p].rfind("<p")?;
                let p_gt = find_from(html, ">", p_open)? + 1;
                let p_close = find_from(html, "</p>", p_gt)?;
                Some(clean_text(&html[p_gt..p_close]))
            })
            .unwrap_or_default();

        hits.push(SearchHit {
            title,
            url: href,
            snippet,
        });
    }
    hits
}

/// 从给定字节偏移起查找子串，返回绝对偏移。
fn find_from(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay.get(from..)
        .and_then(|s| s.find(needle))
        .map(|i| from + i)
}

/// 从 `<a ...`（不含结尾 `>`）开标签切片提取 href 值（支持单双引号与无引号）。
fn extract_href(a_open: &str) -> Option<String> {
    let at = a_open.find("href=")? + "href=".len();
    let rest = &a_open[at..];
    match rest.as_bytes().first().copied() {
        Some(q @ (b'"' | b'\'')) => {
            let qc = q as char;
            let end = rest[1..].find(qc)? + 1;
            Some(rest[1..end].to_string())
        }
        _ => {
            let end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '>')
                .unwrap_or(rest.len());
            Some(rest[..end].to_string())
        }
    }
}

/// 最小 percent-encode：保留 `A-Za-z0-9-_.~`，空格→`+`，其余字节→`%XX`。
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// percent-decode：`%XX`→字节，`+`→空格，非法序列原样保留。按 UTF-8 lossy 还原。
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 去除 HTML 标签（`<...>` 之间全删），并折叠相邻空白。
pub fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// 基本 HTML 实体反转义：`&amp; &lt; &gt; &#39; &quot; &nbsp;` 及数字实体。
pub fn unescape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = input[i..].find(';') {
                let entity = &input[i + 1..i + semi];
                let replacement = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                        u32::from_str_radix(&entity[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                    }
                    _ if entity.starts_with('#') => {
                        entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                    }
                    _ => None,
                };
                if let Some(ch) = replacement {
                    out.push(ch);
                    i += semi + 1;
                    continue;
                }
            }
        }
        // 非实体或无法识别：原样推进一个字节（UTF-8 安全：& 是 ASCII）。
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn utf8_char_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// 内层 HTML → 干净文本：去标签 + 实体反转义 + 折叠空白 + trim。
fn clean_text(inner: &str) -> String {
    let stripped = strip_tags(inner);
    let unescaped = unescape_html(&stripped);
    unescaped.split_whitespace().collect::<Vec<_>>().join(" ")
}
