//! 把 ProviderGateway + 选定 embedding 模型包装成 knowledge 的 Embedder。
use std::sync::Arc;
use crate::knowledge::embed::Embedder;
use crate::provider::ProviderGateway;

pub struct GatewayEmbedder {
    pub gateway: Arc<ProviderGateway>,
    pub model_id: String,
}

impl Embedder for GatewayEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if self.model_id.trim().is_empty() {
            return Err("未选择 embedding 模型".into());
        }
        self.gateway.embed_texts(&self.model_id, texts)
    }
}
