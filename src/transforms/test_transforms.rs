use crate::message::Message;
use crate::transforms::chain::{Transform, Wrapper, TransformChain};
use async_trait::async_trait;
use crate::error::ChainResponse;

#[derive(Debug, Clone)]
pub struct ReturnerTransform {
    pub message: Message
}

#[async_trait]
impl Transform for ReturnerTransform {
    async fn transform(&self, _qd: Wrapper, _t: &TransformChain) -> ChainResponse {
        return Ok(self.message.clone())
    }

    fn get_name(&self) -> &'static str {
        return "returner"
    }
}