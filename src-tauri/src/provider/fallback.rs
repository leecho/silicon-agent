//! 一次性主备降级 helper。
//!
//! emperor 语义：调用主模型；若失败且配置了 `fallback_model`，则**一次性**用备用模型
//! 重试一次（同一轮、不改 history、不再二次降级）。返回最终结果或最后的错误。

use crate::provider::client::{ModelCallRequest, ModelCallResult, ModelClient, ProviderCallError};

/// 调用主模型；失败且有备用模型时，用备用模型（写入 `model_selection`）重试一次。
pub fn call_with_fallback(
    client: &dyn ModelClient,
    request: ModelCallRequest,
) -> Result<ModelCallResult, ProviderCallError> {
    match client.stream_model(request.clone()) {
        Ok(result) => Ok(result),
        Err(primary_err) => match client.fallback_model() {
            Some(fallback) => {
                let mut fallback_request = request;
                fallback_request.model_selection = Some(fallback);
                client.stream_model(fallback_request)
            }
            None => Err(primary_err),
        },
    }
}
