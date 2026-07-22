//! OAuth 2.1（MCP authorization 规范）：发现、动态注册、PKCE 授权码流、token 刷新。
//! 回调监听 127.0.0.1 临时端口；浏览器打开由调用方注入（命令层用 opener 插件）。

use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::provider::secret::FileSecretStore;

/// 持久化在 secret store（key=`{server_id}:oauth`）的 token 与续期所需上下文。
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// unix 秒；None = 未知过期时间（每次都直接用，401 再刷新）。
    pub expires_at: Option<i64>,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

impl std::fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"****")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "****"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_endpoint", &self.token_endpoint)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "****"),
            )
            .finish()
    }
}

/// 授权服务器元数据（RFC 8414 子集）。
#[derive(Debug, Clone, Deserialize)]
pub struct AuthServerMeta {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    /// AS 声明支持的 scope（RFC 8414）。
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    /// AS 支持的 token 端点认证方式（RFC 8414）。缺省视为只支持 `client_secret_basic`（RFC 默认）。
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// 受保护资源（PRM，RFC 9728）声明的 scope。**优先于 AS 的 scopes_supported**——
    /// 它是「这个资源要什么权限」的权威answer（如 Figma 的 `mcp:connect`）。
    /// 非 AS 元数据字段，由 `discover_via_prm` 回填。
    #[serde(skip)]
    pub resource_scopes: Vec<String>,
    /// PRM 声明的 canonical `resource`（RFC 9728）——AS 认的就是这个值。
    /// 非 AS 元数据字段，由 `discover_via_prm` 回填。
    #[serde(skip)]
    pub resource_canonical: Option<String>,
}

impl AuthServerMeta {
    /// 本次授权应请求的 scope（空格分隔）；无从得知则 None（不带 scope 参数）。
    /// 优先用 PRM 的（资源自己声明要什么），退回 AS 的。
    pub fn scope_param(&self) -> Option<String> {
        let src = if !self.resource_scopes.is_empty() {
            &self.resource_scopes
        } else {
            &self.scopes_supported
        };
        if src.is_empty() {
            None
        } else {
            Some(src.join(" "))
        }
    }

    /// RFC 8707 的 `resource` 参数取值，按权威性排序：
    /// ① PRM 的 canonical `resource`（AS 认的就是它）
    /// ② 清单的 `oauth_resource`（PRM 拿不到时，插件作者给的提示）
    /// ③ server_url（兜底；Figma 恰好等于它）
    ///
    /// 取错会导致 token 的 audience 不匹配 → 拿到 token 也调不通（仍 401）。
    pub fn resource_param(&self, manifest_resource: Option<&str>, server_url: &str) -> String {
        self.resource_canonical
            .as_deref()
            .or(manifest_resource)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(server_url)
            .to_string()
    }

    /// DCR 注册时声明的 token 端点认证方式。
    ///
    /// **不能写死 `none`**：有的 AS（如 Figma）压根不支持公开客户端，
    /// 只认 `client_secret_basic`/`client_secret_post`，写死 none 会被拒。
    /// 策略：AS 明确支持 `none` 才用 none（公开客户端更安全、无需存密钥）；
    /// 否则从其声明里挑一个它支持的；都没声明时按 RFC 8414 默认 `client_secret_basic`。
    pub fn dcr_auth_method(&self) -> &str {
        let supported = &self.token_endpoint_auth_methods_supported;
        if supported.is_empty() || supported.iter().any(|m| m == "none") {
            return "none";
        }
        for candidate in ["client_secret_post", "client_secret_basic"] {
            if supported.iter().any(|m| m == candidate) {
                return candidate;
            }
        }
        "client_secret_basic"
    }
}

/// PKCE：返回 (code_verifier, code_challenge)，S256。
pub fn pkce_pair() -> (String, String) {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

/// 已知 PKCE 校验向量（RFC 7636 附录 B）的纯函数版，便于测试。
pub fn s256_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// 发现授权服务器：
/// 1) 对 server URL 发未鉴权 POST，期望 401 + WWW-Authenticate 指向资源元数据（RFC 9728）；
/// 2) 退路：直接取 server origin 的 PRM（`/.well-known/oauth-protected-resource`）；
/// 3) 兜底：直接探测 server origin 的 AS 元数据。
///
/// 每处取 AS 元数据都试两个端点：RFC 8414 的 `oauth-authorization-server`，
/// 再退 OIDC 的 `openid-configuration`（不少 AS 只发后者）。
pub fn discover(server_url: &str) -> Result<AuthServerMeta, String> {
    if let Some(meta) = discover_via_prm(server_url) {
        return Ok(meta);
    }
    let origin = url_origin(server_url)?;
    // 有的服务不回 401 挑战，但仍按 RFC 9728 在固定路径发布 PRM。
    if let Some(meta) = fetch_prm_and_as(&format!("{origin}/.well-known/oauth-protected-resource"))
    {
        return Ok(meta);
    }
    fetch_as_meta_any(origin.trim_end_matches('/'))
        .ok_or_else(|| "无法发现授权服务器（资源元数据与 well-known 探测均失败）".to_string())
}

/// 取 AS 元数据：先 RFC 8414，再退 OIDC discovery。
fn fetch_as_meta_any(base: &str) -> Option<AuthServerMeta> {
    let base = base.trim_end_matches('/');
    fetch_as_meta(&format!("{base}/.well-known/oauth-authorization-server"))
        .or_else(|| fetch_as_meta(&format!("{base}/.well-known/openid-configuration")))
}

/// 拉一份 PRM 文档，据其 `authorization_servers[0]` 取 AS 元数据，并回填资源 scope 与 canonical resource。
fn fetch_prm_and_as(prm_url: &str) -> Option<AuthServerMeta> {
    let prm_resp = crate::http::HttpClient::new()
        .send(crate::http::HttpRequest::get(prm_url))
        .ok()?;
    if !prm_resp.is_success() {
        return None;
    }
    let prm: serde_json::Value = prm_resp.json().ok()?;
    meta_from_prm(&prm)
}

/// PRM 文档 → AS 元数据（含回填资源 scope / canonical resource）。
fn meta_from_prm(prm: &serde_json::Value) -> Option<AuthServerMeta> {
    // 一期：仅取首个 AS
    let as_url = prm
        .get("authorization_servers")?
        .as_array()?
        .first()?
        .as_str()?;
    let mut meta = fetch_as_meta_any(as_url)?;
    // PRM 的 scopes_supported = 「访问这个资源需要什么权限」，权威性高于 AS 的全集。
    meta.resource_scopes = prm
        .get("scopes_supported")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    // canonical resource：AS 认的 audience 就是它，优先于 server_url。
    meta.resource_canonical = prm
        .get("resource")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(meta)
}

fn discover_via_prm(server_url: &str) -> Option<AuthServerMeta> {
    let resp = crate::http::HttpClient::new()
        .send(
            crate::http::HttpRequest::post(server_url)
                .header("Accept", "application/json")
                .string_body("{}"),
        )
        .ok()?;
    let www = if resp.status == 401 {
        resp.header("WWW-Authenticate").map(String::from)
    } else {
        None
    }?;
    // 形如 Bearer resource_metadata="https://..." 或 Bearer resource_metadata=https://...
    let after_key = www.split("resource_metadata=").nth(1)?;
    let url = if after_key.starts_with('"') {
        // 带引号：取引号内内容
        after_key
            .trim_start_matches('"')
            .split('"')
            .next()?
            .to_string()
    } else {
        // 不带引号：取到首个 `,`、空格或行尾为止
        after_key
            .split(|c: char| c == ',' || c.is_whitespace())
            .next()?
            .to_string()
    };
    let prm_resp = crate::http::HttpClient::new()
        .send(crate::http::HttpRequest::get(url))
        .ok()?;
    if !prm_resp.is_success() {
        return None;
    }
    let prm: serde_json::Value = prm_resp.json().ok()?;
    meta_from_prm(&prm)
}

fn fetch_as_meta(meta_url: &str) -> Option<AuthServerMeta> {
    let resp = crate::http::HttpClient::new()
        .send(crate::http::HttpRequest::get(meta_url))
        .ok()?;
    if !resp.is_success() {
        return None;
    }
    resp.json().ok()
}

fn url_origin(u: &str) -> Result<String, String> {
    let parsed = url::Url::parse(u).map_err(|e| format!("URL 不合法：{e}"))?;
    Ok(parsed.origin().ascii_serialization())
}

/// 动态注册（RFC 7591）。失败返回 None（上层退回手填 client_id）。
pub fn dynamic_register(
    meta: &AuthServerMeta,
    redirect_uri: &str,
) -> Option<(String, Option<String>)> {
    let endpoint = meta.registration_endpoint.as_ref()?;
    let mut body = serde_json::json!({
        "client_name": "silicon-worker",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        // 不能写死 none：Figma 这类 AS 不支持公开客户端，会拒绝注册。
        "token_endpoint_auth_method": meta.dcr_auth_method(),
    });
    // 注册时声明所需 scope，否则发出来的 client 可能没被授予该权限。
    if let Some(scope) = meta.scope_param() {
        body["scope"] = serde_json::Value::String(scope);
    }
    let dcr_resp = crate::http::HttpClient::new()
        .send(
            crate::http::HttpRequest::post(endpoint.clone())
                .content_type("application/json")
                .string_body(body.to_string()),
        )
        .ok()?;
    if !dcr_resp.is_success() {
        return None;
    }
    let resp: serde_json::Value = dcr_resp.json().ok()?;
    let id = resp.get("client_id")?.as_str()?.to_string();
    let secret = resp
        .get("client_secret")
        .and_then(|s| s.as_str())
        .map(String::from);
    Some((id, secret))
}

/// 在 127.0.0.1 临时端口等待一次授权回调，返回 (code, state)。
/// 监听在调用前已 bind（端口先定才能拼 redirect_uri）。
///
/// 健壮性说明：
/// - 杂散连接（favicon、探测请求、非 GET、畸形行）回 404 后继续等待，不终止流程。
/// - 请求行支持分段到达：累积读取直到出现 `\r\n`、16KB 上限或读超时/EOF。
pub fn wait_callback(
    listener: std::net::TcpListener,
    timeout: std::time::Duration,
) -> Result<(String, String), String> {
    use std::io::{Read, Write};

    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + timeout;

    loop {
        // 轮询 accept 直到 deadline
        let mut stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() > deadline {
                        return Err("等待浏览器授权超时".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => return Err(format!("回调监听失败：{e}")),
            }
        };

        // 切换为阻塞模式，设单条连接读超时
        stream.set_nonblocking(false).ok();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .ok();

        // 累积读取直到出现首个 \r\n（拿到请求行即可），或 16KB 上限，或读超时/EOF
        const MAX_BUF: usize = 16 * 1024;
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        let mut tmp = [0u8; 512];
        let first_line: Option<String> = loop {
            match stream.read(&mut tmp) {
                Ok(0) => break None, // EOF
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    // 找首个 \r\n
                    if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n") {
                        let line = String::from_utf8_lossy(&buf[..pos]).into_owned();
                        break Some(line);
                    }
                    if buf.len() >= MAX_BUF {
                        break None; // 超限，视为无效
                    }
                }
                Err(_) => break None, // 读超时或其他错误
            }
        };

        let line = match first_line {
            Some(l) => l,
            None => {
                // 无法解析请求行，回 404 继续等
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
        };

        // 解析请求行：仅 GET /callback?... HTTP/1.1 视为回调
        let mut parts = line.splitn(3, ' ');
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");

        if method != "GET" || !path.starts_with("/callback?") {
            // 杂散连接（favicon、探测、非 GET 等）：404 后继续等
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }

        // 解析 query string
        let query = path.split('?').nth(1).unwrap_or_default();
        let mut code = None;
        let mut state = None;
        for kv in query.split('&') {
            let mut it = kv.splitn(2, '=');
            match (it.next(), it.next()) {
                (Some("code"), Some(v)) => code = Some(urldecode(v)),
                (Some("state"), Some(v)) => state = Some(urldecode(v)),
                _ => {}
            }
        }

        // 回复 200
        let body = "授权完成，请返回 silicon-worker。";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
             <html><body>{body}</body></html>"
        );
        let _ = stream.write_all(response.as_bytes());

        return match (code, state) {
            (Some(c), Some(s)) => Ok((c, s)),
            // 缺少 code/state（如用户拒绝授权，AS 回 error 参数）：真正的终止条件
            _ => Err("回调缺少 code/state 参数（授权可能被拒绝）".into()),
        };
    }
}

fn urldecode(s: &str) -> String {
    url::form_urlencoded::parse(format!("k={s}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| s.to_string())
}

/// 已准备好的授权会话：含可展示的 auth_url 与等待回调所需上下文。
pub struct PendingAuth {
    pub auth_url: String,
    listener: std::net::TcpListener,
    verifier: String,
    state: String,
    client_id: String,
    client_secret: Option<String>,
    redirect_uri: String,
    meta: AuthServerMeta,
    /// RFC 8707 的 `resource`（已按 PRM > 清单 > server_url 解析）。
    /// **授权 URL 与 token 交换必须用同一个值**，否则 AS 会拒或发出 audience 不匹配的 token。
    resource: String,
}

/// 同步准备授权：discover → 绑定本地回调端口 → 动态注册(或用手填 client_id) → PKCE → 构造 auth_url。
/// 错误在此立即返回（discover/注册失败等），便于命令层同步反馈。
pub fn prepare_authorization(
    server_url: &str,
    manual_client_id: Option<String>,
    manifest_resource: Option<String>,
) -> Result<PendingAuth, String> {
    let meta = discover(server_url)?;
    // resource 权威性：PRM canonical > 清单 oauth_resource > server_url。取错 → token audience
    // 不匹配 → 拿到 token 也调不通。
    let resource = meta.resource_param(manifest_resource.as_deref(), server_url);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let (client_id, client_secret) = match manual_client_id {
        Some(id) => (id, None),
        None => dynamic_register(&meta, &redirect_uri)
            .ok_or("授权服务器不支持动态注册，请在配置里为该服务填 clientId")?,
    };
    let (verifier, challenge) = pkce_pair();
    let state: String = {
        let bytes: [u8; 16] = rand::thread_rng().gen();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    };
    let mut auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}&resource={}",
        meta.authorization_endpoint,
        urlencode(&client_id),
        urlencode(&redirect_uri),
        challenge,
        state,
        urlencode(&resource),
    );
    // scope 必带：Figma 等资源要求 `mcp:connect`，不带会拿到无权限的 token（调用仍 401）。
    if let Some(scope) = meta.scope_param() {
        auth_url.push_str(&format!("&scope={}", urlencode(&scope)));
    }
    Ok(PendingAuth {
        auth_url,
        listener,
        verifier,
        state,
        client_id,
        client_secret,
        redirect_uri,
        meta,
        resource,
    })
}

/// 后台完成授权：等待浏览器回调（≤300s）→ 校验 state → 用 code 换 token → 存库。
pub fn finish_authorization(
    p: PendingAuth,
    server_id: &str,
    secrets: &FileSecretStore,
) -> Result<(), String> {
    let (code, got_state) = wait_callback(p.listener, std::time::Duration::from_secs(300))?;
    if got_state != p.state {
        return Err("state 校验失败，已拒绝该回调（可能存在 CSRF）".into());
    }
    let tokens = exchange_code(
        &p.meta,
        &p.client_id,
        p.client_secret.as_deref(),
        &code,
        &p.verifier,
        &p.redirect_uri,
        &p.resource,
    )?;
    save_tokens(server_id, secrets, &tokens)
}

fn exchange_code(
    meta: &AuthServerMeta,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    resource: &str,
) -> Result<OAuthTokens, String> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
        ("resource", resource),
    ];
    if let Some(cs) = client_secret {
        form.push(("client_secret", cs));
    }
    let http_resp = crate::http::HttpClient::new()
        .send(crate::http::HttpRequest::post(meta.token_endpoint.clone()).form_body(&form))
        .map_err(|e| format!("换取 token 失败：{e}"))?;
    if !http_resp.is_success() {
        let head: String = http_resp.text().chars().take(200).collect();
        return Err(format!("换取 token 失败：HTTP {} {head}", http_resp.status));
    }
    let resp: serde_json::Value = http_resp.json().map_err(|e| e.to_string())?;
    parse_token_response(resp, &meta.token_endpoint, client_id, client_secret)
}

fn parse_token_response(
    resp: serde_json::Value,
    token_endpoint: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens, String> {
    let access = resp
        .get("access_token")
        .and_then(|t| t.as_str())
        .ok_or("token 响应缺少 access_token")?;
    let expires_at = resp
        .get("expires_in")
        .and_then(|e| e.as_i64())
        .map(|secs| chrono::Utc::now().timestamp() + secs);
    Ok(OAuthTokens {
        access_token: access.to_string(),
        refresh_token: resp
            .get("refresh_token")
            .and_then(|t| t.as_str())
            .map(String::from),
        expires_at,
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
        client_secret: client_secret.map(String::from),
    })
}

fn save_tokens(server_id: &str, secrets: &FileSecretStore, t: &OAuthTokens) -> Result<(), String> {
    secrets.set(
        &format!("{server_id}:oauth"),
        &serde_json::to_string(t).map_err(|e| e.to_string())?,
    )
}

pub fn load_tokens(server_id: &str, secrets: &FileSecretStore) -> Option<OAuthTokens> {
    let raw = secrets.read(&format!("{server_id}:oauth")).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 取可用的 access token：临期（<60s）且有 refresh_token 则先刷新。
/// 返回 Err 表示需要用户重新授权。
pub fn ensure_fresh_token(server_id: &str, secrets: &FileSecretStore) -> Result<String, String> {
    let tokens = load_tokens(server_id, secrets).ok_or("尚未授权")?;
    let expiring = tokens
        .expires_at
        .map(|at| at - chrono::Utc::now().timestamp() < 60)
        .unwrap_or(false);
    if !expiring {
        return Ok(tokens.access_token);
    }
    refresh(server_id, secrets, &tokens)
}

/// 用 refresh_token 换新 access token；失败即要求重新授权。
pub fn refresh(
    server_id: &str,
    secrets: &FileSecretStore,
    tokens: &OAuthTokens,
) -> Result<String, String> {
    let rt = tokens
        .refresh_token
        .as_deref()
        .ok_or("无 refresh_token，需要重新授权")?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", rt),
        ("client_id", tokens.client_id.as_str()),
    ];
    if let Some(cs) = tokens.client_secret.as_deref() {
        form.push(("client_secret", cs));
    }
    let http_resp = crate::http::HttpClient::new()
        .send(crate::http::HttpRequest::post(tokens.token_endpoint.clone()).form_body(&form))
        .map_err(|_| "刷新 token 失败，需要重新授权".to_string())?;
    if !http_resp.is_success() {
        return Err("刷新 token 失败，需要重新授权".to_string());
    }
    let resp: serde_json::Value = http_resp.json().map_err(|e| e.to_string())?;
    let mut new_tokens = parse_token_response(
        resp,
        &tokens.token_endpoint,
        &tokens.client_id,
        tokens.client_secret.as_deref(),
    )?;
    // 部分 AS 刷新时不回 refresh_token：沿用旧的。
    if new_tokens.refresh_token.is_none() {
        new_tokens.refresh_token = tokens.refresh_token.clone();
    }
    save_tokens(server_id, secrets, &new_tokens)?;
    Ok(new_tokens.access_token)
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    /// 用 Figma MCP 的**真实**元数据钉住 scope / DCR 认证方式的行为。
    /// （2026-07-13 实探 https://mcp.figma.com/mcp 得到，见 T104 §5.3 风险①已解。）
    mod figma_real_metadata {
        use crate::mcp::auth::AuthServerMeta;

        fn figma_meta() -> AuthServerMeta {
            // api.figma.com/.well-known/oauth-authorization-server 的真实响应（节选）
            let raw = r#"{
                "issuer":"https://api.figma.com",
                "authorization_endpoint":"https://www.figma.com/oauth/mcp",
                "token_endpoint":"https://api.figma.com/v1/oauth/token",
                "registration_endpoint":"https://api.figma.com/v1/oauth/mcp/register",
                "code_challenge_methods_supported":["S256"],
                "token_endpoint_auth_methods_supported":["client_secret_basic","client_secret_post"],
                "scopes_supported":["mcp:connect"]
            }"#;
            let mut m: AuthServerMeta = serde_json::from_str(raw).expect("解析 AS 元数据");
            // PRM（mcp.figma.com/.well-known/oauth-protected-resource）声明的资源 scope
            m.resource_scopes = vec!["mcp:connect".to_string()];
            m
        }

        #[test]
        fn requests_the_scope_figma_demands() {
            // 不带 scope 会拿到无 mcp:connect 权限的 token → MCP 调用仍 401。
            assert_eq!(figma_meta().scope_param().as_deref(), Some("mcp:connect"));
        }

        #[test]
        fn prm_scope_wins_over_as_scope() {
            // PRM 说「访问我要什么权限」，比 AS 的全集更权威。
            let mut m = figma_meta();
            m.scopes_supported = vec!["everything".into(), "else".into()];
            m.resource_scopes = vec!["mcp:connect".into()];
            assert_eq!(m.scope_param().as_deref(), Some("mcp:connect"));
        }

        #[test]
        fn does_not_register_as_public_client_when_as_forbids_it() {
            // Figma 的 token_endpoint_auth_methods_supported 里**没有 none**，
            // 写死 "none" 会被拒绝注册。
            assert_eq!(figma_meta().dcr_auth_method(), "client_secret_post");
        }
    }

    #[test]
    fn resource_priority_prm_then_manifest_then_server_url() {
        // 取错 resource → token 的 audience 不匹配 → 拿到 token 也调不通（仍 401）。
        let mut m = super::AuthServerMeta {
            authorization_endpoint: "https://as/auth".into(),
            token_endpoint: "https://as/token".into(),
            registration_endpoint: None,
            scopes_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            resource_scopes: vec![],
            resource_canonical: None,
        };

        // ③ 都没有 → 兜底 server_url
        assert_eq!(
            m.resource_param(None, "https://mcp.example.com/mcp"),
            "https://mcp.example.com/mcp"
        );

        // ② 清单声明了 oauth_resource → 用它（PRM 拿不到时）
        assert_eq!(
            m.resource_param(
                Some("https://api.example.com/v1"),
                "https://mcp.example.com/mcp"
            ),
            "https://api.example.com/v1"
        );

        // ① PRM 的 canonical resource 最权威 —— 压过清单
        m.resource_canonical = Some("https://canonical.example.com/mcp".into());
        assert_eq!(
            m.resource_param(
                Some("https://api.example.com/v1"),
                "https://mcp.example.com/mcp"
            ),
            "https://canonical.example.com/mcp"
        );

        // 空串不算数（别把 resource 发成空）
        m.resource_canonical = None;
        assert_eq!(
            m.resource_param(Some("   "), "https://mcp.example.com/mcp"),
            "https://mcp.example.com/mcp"
        );
    }

    #[test]
    fn scope_absent_when_metadata_declares_none() {
        // 没声明 scope 的 AS：不该硬塞一个 scope 参数上去。
        let m = super::AuthServerMeta {
            authorization_endpoint: "https://as/auth".into(),
            token_endpoint: "https://as/token".into(),
            registration_endpoint: None,
            scopes_supported: vec![],
            token_endpoint_auth_methods_supported: vec![],
            resource_scopes: vec![],
            resource_canonical: None,
        };
        assert!(m.scope_param().is_none());
        // 未声明认证方式时按公开客户端注册（PKCE 已足够，且无需存密钥）。
        assert_eq!(m.dcr_auth_method(), "none");
    }

    #[test]
    fn prefers_public_client_when_as_supports_none() {
        let m = super::AuthServerMeta {
            authorization_endpoint: "https://as/auth".into(),
            token_endpoint: "https://as/token".into(),
            registration_endpoint: None,
            scopes_supported: vec![],
            token_endpoint_auth_methods_supported: vec!["none".into(), "client_secret_post".into()],
            resource_scopes: vec![],
            resource_canonical: None,
        };
        assert_eq!(m.dcr_auth_method(), "none", "AS 支持 none 就用公开客户端");
    }

    use super::*;

    #[test]
    fn pkce_matches_rfc7636_vector() {
        // RFC 7636 附录 B 已知向量。
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            s256_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_pair_is_urlsafe_and_consistent() {
        let (v, c) = pkce_pair();
        assert!(!v.contains('+') && !v.contains('/') && !v.contains('='));
        assert_eq!(s256_challenge(&v), c);
    }

    #[test]
    fn token_response_parses_and_expiry_computed() {
        let t = parse_token_response(
            serde_json::json!({"access_token":"at","refresh_token":"rt","expires_in":3600}),
            "https://as/token",
            "cid",
            None,
        )
        .unwrap();
        assert_eq!(t.access_token, "at");
        assert!(t.expires_at.unwrap() > chrono::Utc::now().timestamp() + 3500);
    }

    #[test]
    fn wait_callback_parses_code_and_state() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::Write;
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(b"GET /callback?code=abc&state=xyz HTTP/1.1\r\n\r\n")
                .unwrap();
            let mut buf = [0u8; 1024];
            use std::io::Read;
            let _ = s.read(&mut buf);
        });
        let (code, state) = wait_callback(listener, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!((code.as_str(), state.as_str()), ("abc", "xyz"));
    }

    /// T1：分两半写入请求行，中间 sleep 50ms，断言解析成功。
    #[test]
    fn wait_callback_handles_split_request() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::Write;
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            // 分两半写入
            s.write_all(b"GET /callback?code=abc").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            s.write_all(b"&state=xyz HTTP/1.1\r\n\r\n").unwrap();
            let mut buf = [0u8; 1024];
            use std::io::Read;
            let _ = s.read(&mut buf);
        });
        let (code, state) = wait_callback(listener, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!((code.as_str(), state.as_str()), ("abc", "xyz"));
    }

    /// T2：杂散连接（favicon）不终止流程，第二条真回调正常返回。
    #[test]
    fn stray_connection_does_not_abort_wait() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            // 第一条：杂散连接，发 favicon 请求，读掉 404 响应后关闭
            {
                let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n").unwrap();
                let mut buf = [0u8; 256];
                let _ = s.read(&mut buf);
                // s 在此作用域结束时 drop，连接关闭
            }
            // 第二条：真正的回调
            {
                let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.write_all(b"GET /callback?code=real_code&state=real_state HTTP/1.1\r\n\r\n")
                    .unwrap();
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
            }
        });
        let (code, state) = wait_callback(listener, std::time::Duration::from_secs(5)).unwrap();
        assert_eq!((code.as_str(), state.as_str()), ("real_code", "real_state"));
    }
}
