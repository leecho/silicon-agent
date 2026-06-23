use silicon_agent::skill::frontmatter::{parse_frontmatter, strip_frontmatter};

#[test]
fn parses_name_and_description() {
    let md = "---\nname: my-skill\ndescription: 一句话说明\n---\n正文第一行\n";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.name, "my-skill");
    assert_eq!(fm.description, "一句话说明");
}

#[test]
fn strips_quotes_and_whitespace() {
    let md = "---\nname:  \"quoted-name\" \ndescription: '带引号'\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.name, "quoted-name");
    assert_eq!(fm.description, "带引号");
}

#[test]
fn missing_name_is_error() {
    let md = "---\ndescription: 只有描述\n---\nbody";
    assert!(parse_frontmatter(md).is_err());
}

#[test]
fn no_frontmatter_is_error() {
    let md = "# 直接是正文\n没有 frontmatter";
    assert!(parse_frontmatter(md).is_err());
}

#[test]
fn prefers_description_zh_when_present() {
    let md = "---\nname: docx\ndescription: English desc\ndescription_zh: 中文描述\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.description, "中文描述", "应优先取 description_zh");
}

#[test]
fn falls_back_to_description_when_zh_missing() {
    let md = "---\nname: docx\ndescription: English desc\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.description, "English desc");
}

#[test]
fn falls_back_to_description_when_zh_empty() {
    let md = "---\nname: docx\ndescription: English desc\ndescription_zh: \n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.description, "English desc", "空 description_zh 应回退");
}

#[test]
fn missing_description_defaults_empty() {
    let md = "---\nname: only-name\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert_eq!(fm.description, "");
}

#[test]
fn defaults_user_invocable_true_and_optional_fields_none() {
    let md = "---\nname: x\ndescription: d\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert!(fm.user_invocable, "默认应可见");
    assert_eq!(fm.argument_hint, None);
    assert_eq!(fm.version, None);
}

#[test]
fn parses_user_invocable_false_and_argument_hint_and_version() {
    let md = "---\nname: kb\ndescription: d\nuser-invocable: false\nargument-hint: 上传合同文件\nversion: 1.2.0\n---\nbody";
    let fm = parse_frontmatter(md).expect("parse");
    assert!(!fm.user_invocable, "显式 false 应隐藏");
    assert_eq!(fm.argument_hint.as_deref(), Some("上传合同文件"));
    assert_eq!(fm.version.as_deref(), Some("1.2.0"));
}

#[test]
fn strip_returns_body_only() {
    let md = "---\nname: x\n---\n正文行1\n正文行2\n";
    assert_eq!(strip_frontmatter(md), "正文行1\n正文行2\n");
}

#[test]
fn strip_without_frontmatter_returns_original() {
    let md = "没有 frontmatter 的正文";
    assert_eq!(strip_frontmatter(md), "没有 frontmatter 的正文");
}
