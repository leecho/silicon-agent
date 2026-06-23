use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use crate::tools::web_search::{strip_tags, unescape_html};
use crate::tools::Tool;

/// 与 web_search 一致的浏览器 UA，降低被简单反爬拦截的概率。
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 默认/最大返回字符数。正文常很长，默认截断到 10k，模型可按需调大。
const DEFAULT_MAX_CHARS: usize = 10_000;
const MAX_MAX_CHARS: usize = 50_000;

/// `web_fetch` 工具：抓取指定 URL 的正文并转为可读纯文本。
///
/// 与 `web_search` 互补：search 给链接，fetch 读正文。只读网络 GET，无副作用，故 risk = Safe。
/// 默认拒绝回环/私有网段，避免被诱导探测本机/内网服务（SSRF）。
pub struct WebFetch;

impl Tool for WebFetch {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn label(&self) -> &str {
        "抓取网页"
    }

    fn description(&self) -> &str {
        "抓取指定 http(s) URL 的网页正文并返回可读纯文本（自动去标签/脚本/样式）。\
         配合 web_search 使用：先搜到链接，再用本工具读其正文。出于安全，拒绝本机/内网地址。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "要抓取的 http(s) 链接"},
                "max_chars": {
                    "type": "integer",
                    "minimum": 500,
                    "maximum": MAX_MAX_CHARS,
                    "description": "返回正文最多字符数（默认 10000，范围 500..50000，超出截断）"
                }
            },
            "required": ["url"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        true
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("缺少 url")?;
        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|v| (v as usize).clamp(500, MAX_MAX_CHARS))
            .unwrap_or(DEFAULT_MAX_CHARS);

        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("仅支持 http/https 链接".into());
        }
        // SSRF 防护：拒绝回环/私有/链路本地等非公网目标。
        match extract_host(url) {
            Some(host) if is_blocked_host(&host) => {
                return Err(format!(
                    "出于安全，拒绝抓取本机/内网地址（{host}）。仅支持公网 http(s) 资源。"
                ));
            }
            None => return Err("无法解析 URL 主机名".into()),
            _ => {}
        }

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(20))
            .build();
        let resp = match agent
            .get(url)
            .set("User-Agent", BROWSER_UA)
            .set("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .call()
        {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, _)) => return Err(fetch_failed(&format!("HTTP {code}"))),
            Err(err) => return Err(fetch_failed(&err.to_string())),
        };

        let final_url = resp.get_url().to_string();
        let is_html = resp.content_type().contains("html");
        let body = resp
            .into_string()
            .map_err(|e| fetch_failed(&format!("读取响应出错: {e}")))?;

        let text = if is_html || looks_like_html(&body) {
            html_to_text(&body)
        } else {
            normalize_text(&body)
        };
        if text.is_empty() {
            return Err(fetch_failed("页面无可提取的文本内容"));
        }

        let (shown, truncated) = truncate_chars(&text, max_chars);
        let mut out = format!("【已抓取 {final_url}】\n\n{shown}");
        if truncated {
            out.push_str(&format!(
                "\n\n[内容已截断：原文约 {} 字符，仅显示前 {} 字符；如需更多请调大 max_chars 或换更精确的来源]",
                text.chars().count(),
                max_chars
            ));
        }
        Ok(out)
    }
}

/// 抓取失败时给模型的明确指引：别死磕同一地址，改走 web_search 或基于已有知识作答。
fn fetch_failed(detail: &str) -> String {
    format!(
        "抓取失败（{detail}）。请确认该 URL 可公开访问，或改用 web_search 获取信息；\
         不要反复重试同一地址。"
    )
}

/// 从 URL 提取主机名（去 scheme、userinfo、path/query/fragment 与端口）。IPv6 字面量保留方括号。
fn extract_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .filter(|s| !s.is_empty())?;
    // 去掉 userinfo（user:pass@host）。
    let authority = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    // IPv6 字面量：[::1]:8080 → [::1]。
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        return Some(format!("[{}]", &rest[..end]));
    }
    // 普通主机：去掉 :port。
    let host = authority.split(':').next().filter(|s| !s.is_empty())?;
    Some(host.to_string())
}

/// 是否为应拒绝的主机：localhost 系列、IP 字面量落在回环/私有/链路本地等范围。
fn is_blocked_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty()
        || h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".internal")
    {
        return true;
    }
    // IPv6 字面量（带方括号）。
    if let Some(inner) = h.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return inner
            .parse::<Ipv6Addr>()
            .map(|ip| is_blocked_ipv6(&ip))
            .unwrap_or(true);
    }
    if let Ok(ip) = h.parse::<Ipv4Addr>() {
        return is_blocked_ipv4(&ip);
    }
    if let Ok(ip) = h.parse::<Ipv6Addr>() {
        return is_blocked_ipv6(&ip);
    }
    false
}

fn is_blocked_ipv4(ip: &Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        // CGNAT 100.64.0.0/10。
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64)
}

fn is_blocked_ipv6(ip: &Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    // IPv4-mapped（::ffff:a.b.c.d）落到 v4 判定。
    if let Some(v4) = ip.to_ipv4() {
        return is_blocked_ipv4(&v4);
    }
    let seg0 = ip.segments()[0];
    // 唯一本地 fc00::/7 与链路本地 fe80::/10。
    (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
}

/// 无 content-type 时的兜底判断：开头疑似 HTML。
fn looks_like_html(body: &str) -> bool {
    let head = body.trim_start().to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.contains("<body")
}

/// HTML → 可读纯文本：去 script/style/noscript 块 → 块级标签转换行 → 去标签 → 反转义 → 规整空白。
fn html_to_text(html: &str) -> String {
    let mut s = remove_block(html, "script");
    s = remove_block(&s, "style");
    s = remove_block(&s, "noscript");
    s = insert_line_breaks(&s);
    let stripped = strip_tags(&s);
    let unescaped = unescape_html(&stripped);
    normalize_text(&unescaped)
}

/// 删除 `<tag ...>...</tag>` 整块（含内容）。大小写不敏感地定位，按原文切片保留其余内容。
/// 锚点为 ASCII，切点落在字符边界，UTF-8 安全。未闭合则丢弃其后全部。
fn remove_block(input: &str, tag: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(rel) = lower[i..].find(&open) {
        let start = i + rel;
        out.push_str(&input[i..start]);
        match lower[start..].find(&close) {
            Some(crel) => i = start + crel + close.len(),
            None => {
                i = input.len();
                break;
            }
        }
    }
    out.push_str(&input[i..]);
    out
}

/// 常见块级闭合标签/换行标签替换为换行，让去标签后保留段落结构。
fn insert_line_breaks(html: &str) -> String {
    let mut s = html.to_string();
    for t in [
        "</p>", "</div>", "</li>", "</tr>", "</h1>", "</h2>", "</h3>", "</h4>", "</h5>", "</h6>",
        "<br>", "<br/>", "<br />",
    ] {
        s = s.replace(t, "\n");
    }
    s
}

/// 规整空白：逐行折叠行内空白；连续空行压成一个；首尾 trim。
fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0u32;
    for line in text.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            blank += 1;
            if blank <= 1 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(&collapsed);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// 按字符数截断，返回（展示文本, 是否截断）。切点落在字符边界。
fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    match text.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => (text[..byte_idx].to_string(), true),
        None => (text.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_host_strips_scheme_userinfo_port_path() {
        assert_eq!(
            extract_host("https://example.com/a/b?x=1"),
            Some("example.com".into())
        );
        assert_eq!(
            extract_host("http://user:pass@host.test:8080/p"),
            Some("host.test".into())
        );
        assert_eq!(extract_host("https://[::1]:9000/x"), Some("[::1]".into()));
        assert_eq!(extract_host("https://10.0.0.5"), Some("10.0.0.5".into()));
        assert_eq!(extract_host("https://"), None);
    }

    #[test]
    fn blocks_loopback_private_and_localhost() {
        for h in [
            "localhost",
            "foo.localhost",
            "svc.local",
            "db.internal",
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.1",
            "172.16.5.5",
            "169.254.1.1",
            "0.0.0.0",
            "100.64.0.1",
            "[::1]",
            "[fe80::1]",
            "[fc00::1]",
            "[::ffff:127.0.0.1]",
        ] {
            assert!(is_blocked_host(h), "应拒绝: {h}");
        }
    }

    #[test]
    fn allows_public_hosts() {
        for h in [
            "example.com",
            "8.8.8.8",
            "1.1.1.1",
            "[2606:4700::1111]",
            "172.32.0.1",
        ] {
            assert!(!is_blocked_host(h), "应放行: {h}");
        }
    }

    #[test]
    fn html_to_text_drops_scripts_styles_and_keeps_structure() {
        let html = "<html><head><style>.a{color:red}</style>\
            <script>var x=1;alert('no')</script></head>\
            <body><h1>标题</h1><p>第一段 &amp; 内容</p><p>第二段</p></body></html>";
        let text = html_to_text(html);
        assert!(!text.contains("alert"), "脚本不应泄漏: {text}");
        assert!(!text.contains("color:red"), "样式不应泄漏: {text}");
        assert!(text.contains("标题"));
        assert!(text.contains("第一段 & 内容"));
        assert!(text.contains("第二段"));
        // 块级标签转换行：标题与段落不黏连。
        assert!(text.contains("标题\n"));
    }

    #[test]
    fn truncate_respects_char_boundary_and_flags() {
        let (s, t) = truncate_chars("héllo世界", 4);
        assert_eq!(s, "héll");
        assert!(t);
        let (s2, t2) = truncate_chars("abc", 10);
        assert_eq!(s2, "abc");
        assert!(!t2);
    }
}
