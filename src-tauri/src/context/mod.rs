//! 上下文装配 owner 模块：系统提示拼装与上下文压缩。
//!
//! backend-engineering-policy 把 `context/**` 指派给「Memory、ContextAssembly 和上下文压缩」。
//! 这里承载「怎么装配 / 怎么压缩」的纯逻辑与受控读写；engine 只负责「何时装配 / 何时压缩」。
//! 本模块不持有任何 store，依赖（SessionStore / dyn ModelClient / ResolvedModel）均由调用方传入。

pub mod compaction;
pub mod prompt;
