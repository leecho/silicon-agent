//! MCP client 子系统：连接外部 MCP server，把其 tools 映射为一等 Tool。
//! 设计见 docs/04-specs/2026-06-13-mcp-client-design.md。

pub mod auth;
pub mod client;
pub mod json;
pub mod manager;
pub mod proxy;
pub mod store;
pub mod transport;
pub mod transport_http;
pub mod transport_sse;
pub mod transport_stdio;
pub mod types;

pub use manager::McpService;
