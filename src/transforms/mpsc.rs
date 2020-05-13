use crate::transforms::chain::{Transform, ChainResponse, Wrapper, TransformChain};
use tokio::sync::mpsc::{Sender, Receiver, channel};

use async_trait::async_trait;
use crate::message::{Message, QueryResponse};
use tokio::task::JoinHandle;
use crate::transforms::kafka_destination::KafkaDestination;
use tokio::sync::mpsc::error::RecvError;
use tokio::runtime::Handle;

pub struct AsyncMpsc {
    name: &'static str,
    tx: Sender<Message>,
    rx_handle: JoinHandle<Result<(), RecvError>>
}

#[derive(Debug, Clone)]
pub struct AsyncMpscForwarder {
    name: &'static str,
    tx: Sender<Message>,
}

#[derive(Debug, Clone)]
pub struct AsyncMpscTee {
    name: &'static str,
    tx: Sender<Message>,
}

impl AsyncMpsc {
    fn test_tee_loop(mut rx: Receiver<Message>, chain: TransformChain) -> JoinHandle<Result<(), RecvError>> {
        Handle::current().spawn(async move {
            // let noop_transformer = NoOp::new();
            let printer_transform = KafkaDestination::new();
            // let printer_transform = Printer::new();
            //TODO provide a way to build the chain from config externally
            loop {
                if let Some(m) = rx.recv().await {
                    let w: Wrapper = Wrapper::new(m.clone());
                    chain.process_request(w).await;
                }
            }
        })
    }

    pub fn new(chain: TransformChain) -> AsyncMpsc {
        let (tx, rx) = channel::<Message>(5);
        return AsyncMpsc {
            name: "AsyncMpsc",
            tx,
            rx_handle: AsyncMpsc::test_tee_loop(rx, chain)
        };
    }

    pub fn get_async_mpsc_forwarder(&self) -> AsyncMpscForwarder {
        AsyncMpscForwarder{
            name: "Forward",
            tx: self.tx.clone(),
        }
    }

    pub fn get_async_mpsc_tee(&self) -> AsyncMpscTee {
        AsyncMpscTee{
            name: "Tee",
            tx: self.tx.clone(),
        }
    }
}


#[async_trait]
impl Transform for AsyncMpscForwarder {
    async fn transform(&self, mut qd: Wrapper, t: & TransformChain) -> ChainResponse {
        self.tx.clone().send(qd.message).await;
        return ChainResponse::Ok(Message::Response(QueryResponse::empty()));
    }

    fn get_name(&self) -> &'static str {
        self.name
    }
}


#[async_trait]
impl Transform for AsyncMpscTee {
    async fn transform(&self, mut qd: Wrapper, t: & TransformChain) -> ChainResponse {
        let m = qd.message.clone();
        self.tx.clone().send(m).await;
        self.call_next_transform(qd, t).await
    }

    fn get_name(&self) -> &'static str {
        self.name
    }
}
