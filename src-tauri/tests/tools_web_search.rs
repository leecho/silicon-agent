//! web_search 解析纯函数单测（不触网）。
//!
//! 后端为 Bing（`https://cn.bing.com/search`）。真实结果块结构：标题
//! `<h2><a href="真实URL">标题</a></h2>`（href 即目标站点，无需 uddg 解包），摘要
//! `<p class="b_lineclamp2">摘要</p>`。下面样本含两条结果，标题/摘要里故意带 HTML 标签
//! (`<strong>`) 与实体 (`&amp; &#39;`) 以验证清洗逻辑。

use silicon_worker::tools::web_search::{
    parse_bing_results, percent_decode, percent_encode, strip_tags, unescape_html,
};

const SAMPLE: &str = r###"
<ol id="b_results">
  <li class="b_algo">
    <h2><a href="https://www.rust-lang.org/" h="ID=SERP,1.1">The Rust <strong>Programming</strong> Language</a></h2>
    <div class="b_caption"><p class="b_lineclamp2">A language empowering everyone &amp; building <strong>reliable</strong> &#39;software&#39;.</p></div>
  </li>
  <li class="b_algo">
    <h2><a href="https://doc.rust-lang.org/book/">The Rust Programming Language - The Book</a></h2>
    <div class="b_caption"><p class="b_lineclamp2">Use Rust to build &lt;fast&gt; CLI tools and web apps.</p></div>
  </li>
</ol>
"###;

#[test]
fn parses_title_url_snippet_and_cleans_html() {
    let hits = parse_bing_results(SAMPLE, 5);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].title, "The Rust Programming Language");
    assert_eq!(hits[0].url, "https://www.rust-lang.org/");
    assert_eq!(
        hits[0].snippet,
        "A language empowering everyone & building reliable 'software'."
    );
    assert_eq!(hits[1].title, "The Rust Programming Language - The Book");
    assert_eq!(hits[1].url, "https://doc.rust-lang.org/book/");
    assert_eq!(
        hits[1].snippet,
        "Use Rust to build <fast> CLI tools and web apps."
    );
}

#[test]
fn respects_max_results() {
    let hits = parse_bing_results(SAMPLE, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url, "https://www.rust-lang.org/");
}

#[test]
fn ignores_non_http_and_empty_titles() {
    // 导航/相关搜索的非结果 h2（相对链接或锚点）不应被收录。
    let junk = r###"<h2><a href="/account">登录</a></h2><h2><a href="#top"></a></h2>"###;
    assert!(parse_bing_results(junk, 5).is_empty());
    assert!(parse_bing_results("<html><body>no results</body></html>", 5).is_empty());
    assert!(parse_bing_results("", 5).is_empty());
}

#[test]
fn result_without_snippet_still_captured_with_empty_snippet() {
    let no_snip =
        r###"<li class="b_algo"><h2><a href="https://example.com/">Example</a></h2></li>"###;
    let hits = parse_bing_results(no_snip, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Example");
    assert_eq!(hits[0].url, "https://example.com/");
    assert_eq!(hits[0].snippet, "");
}

#[test]
fn strip_tags_removes_markup() {
    assert_eq!(strip_tags("a<b>bold</b>c"), "aboldc");
    assert_eq!(strip_tags("<a href=\"x\">link</a>"), "link");
    assert_eq!(strip_tags("plain"), "plain");
    assert_eq!(strip_tags("<br/>"), "");
}

#[test]
fn unescape_html_handles_entities() {
    assert_eq!(unescape_html("a &amp; b"), "a & b");
    assert_eq!(unescape_html("&lt;tag&gt;"), "<tag>");
    assert_eq!(
        unescape_html("it&#39;s &quot;quoted&quot;"),
        "it's \"quoted\""
    );
    assert_eq!(unescape_html("&#x41;&#66;"), "AB");
    assert_eq!(unescape_html("a &unknown; b"), "a &unknown; b");
    assert_eq!(unescape_html("100% done"), "100% done");
}

#[test]
fn percent_encode_decode_round_trip() {
    let s = "福建 招投标 & co";
    let enc = percent_encode(s);
    assert!(!enc.contains(' '));
    assert_eq!(percent_decode(&enc), s);
}
