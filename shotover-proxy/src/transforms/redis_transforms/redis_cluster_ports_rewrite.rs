use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use redis_protocol::resp2::prelude::Frame;
use serde::Deserialize;

use crate::config::topology::TopicHolder;
use crate::error::ChainResponse;
use crate::protocols::RawFrame;
use crate::transforms::{Transform, Transforms, TransformsFromConfig, Wrapper};

#[derive(Deserialize, Debug, Clone)]
pub struct RedisClusterPortsRewriteConfig {
    pub new_port: u16,
}

#[async_trait]
impl TransformsFromConfig for RedisClusterPortsRewriteConfig {
    async fn get_source(&self, _topics: &TopicHolder) -> Result<Transforms> {
        Ok(Transforms::RedisClusterPortsRewrite(
            RedisClusterPortsRewrite::new(self.new_port),
        ))
    }
}

#[derive(Clone)]
pub struct RedisClusterPortsRewrite {
    new_port: u16,
}

impl RedisClusterPortsRewrite {
    pub fn new(new_port: u16) -> Self {
        RedisClusterPortsRewrite { new_port }
    }
}

#[async_trait]
impl Transform for RedisClusterPortsRewrite {
    async fn transform<'a>(&'a mut self, message_wrapper: Wrapper<'a>) -> ChainResponse {
        // Find the indices of cluster slot messages
        let cluster_slots_indices = message_wrapper
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| is_cluster_slots(&m.original))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();

        let mut response = message_wrapper.call_next_transform().await?;

        // Rewrite the ports in the cluster slots responses
        for i in cluster_slots_indices {
            rewrite_port(&mut response[i].original, self.new_port)
                .context("failed to rewrite CLUSTER SLOTS port")?;
        }

        Ok(response)
    }

    fn get_name(&self) -> &'static str {
        "RedisClusterPortsRewrite"
    }
}

/// Rewrites the ports of a response to a CLUSTER SLOTS message to `new_port`
fn rewrite_port(frame: &mut RawFrame, new_port: u16) -> Result<()> {
    if let RawFrame::Redis(Frame::Array(ref mut array)) = frame {
        for elem in array.iter_mut() {
            if let Frame::Array(slot) = elem {
                for (index, mut frame) in slot.iter_mut().enumerate() {
                    match (index, &mut frame) {
                        (0..=1, _) => {}
                        (_, Frame::Array(target)) => match target.as_mut_slice() {
                            [Frame::BulkString(_ip), Frame::Integer(port), ..] => {
                                *port = new_port.into();
                            }
                            _ => bail!("expected host-port in slot map but was: {:?}", frame),
                        },
                        _ => bail!("unexpected value in slot map: {:?}", frame),
                    }
                }
            };
        }
    };

    Ok(())
}

/// Determines if the supplied Redis Frame is a `CLUSTER SLOTS` request
fn is_cluster_slots(frame: &RawFrame) -> bool {
    let args = if let RawFrame::Redis(Frame::Array(array)) = frame {
        array
            .iter()
            .map(|f| match f {
                Frame::BulkString(b) => Some(b.to_ascii_uppercase()),
                _ => None,
            })
            .take_while(Option::is_some)
            .map(Option::unwrap)
    } else {
        return false;
    };

    args.eq([b"CLUSTER", b"SLOTS" as &[u8]])
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocols::redis_codec::{DecodeType, RedisCodec};
    use crate::transforms::redis_transforms::redis_sink_cluster::parse_slots;
    use tokio_util::codec::Decoder;

    #[test]
    fn test_is_cluster_slots() {
        let combos = [
            (b"cluster", b"slots"),
            (b"CLUSTER", b"SLOTS"),
            (b"cluster", b"SLOTS"),
            (b"CLUSTER", b"slots"),
        ];

        for combo in combos {
            let frame = RawFrame::Redis(Frame::Array(vec![
                Frame::BulkString(combo.0.to_vec()),
                Frame::BulkString(combo.1.to_vec()),
            ]));
            assert!(is_cluster_slots(&frame));
        }

        let frame = RawFrame::Redis(Frame::Array(vec![
            Frame::BulkString(b"GET".to_vec()),
            Frame::BulkString(b"key1".to_vec()),
        ]));

        assert!(!is_cluster_slots(&frame));
    }

    #[test]
    fn test_rewrite_port() {
        let slots_pcap: &[u8] = b"*3\r\n*4\r\n:10923\r\n:16383\r\n*3\r\n$12\r\n192.168.80.6\r\n:6379\r\n$40\r\n3a7c357ed75d2aa01fca1e14ef3735a2b2b8ffac\r\n*3\r\n$12\r\n192.168.80.3\r\n:6379\r\n$40\r\n77c01b0ddd8668fff05e3f6a8aaf5f3ccd454a79\r\n*4\r\n:5461\r\n:10922\r\n*3\r\n$12\r\n192.168.80.5\r\n:6379\r\n$40\r\n969c6215d064e68593d384541ceeb57e9520dbed\r\n*3\r\n$12\r\n192.168.80.2\r\n:6379\r\n$40\r\n3929f69990a75be7b2d49594c57fe620862e6fd6\r\n*4\r\n:0\r\n:5460\r\n*3\r\n$12\r\n192.168.80.7\r\n:6379\r\n$40\r\n15d52a65d1fc7a53e34bf9193415aa39136882b2\r\n*3\r\n$12\r\n192.168.80.4\r\n:6379\r\n$40\r\ncd023916a3528fae7e606a10d8289a665d6c47b0\r\n";
        let mut codec = RedisCodec::new(DecodeType::Response, 3);
        let mut raw_frame = codec
            .decode(&mut slots_pcap.into())
            .unwrap()
            .unwrap()
            .pop()
            .unwrap()
            .original;

        rewrite_port(&mut raw_frame, 2004).unwrap();

        let slots_frames = if let RawFrame::Redis(Frame::Array(frames)) = raw_frame.clone() {
            frames
        } else {
            panic!("bad input: {:?}", raw_frame)
        };

        let slots = parse_slots(&slots_frames).unwrap();

        let nodes = vec![
            "192.168.80.2:2004",
            "192.168.80.3:2004",
            "192.168.80.4:2004",
            "192.168.80.5:2004",
            "192.168.80.6:2004",
            "192.168.80.7:2004",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let masters = vec![
            (5460u16, "192.168.80.7:2004".to_string()),
            (10922u16, "192.168.80.5:2004".to_string()),
            (16383u16, "192.168.80.6:2004".to_string()),
        ];

        let replicas = vec![
            (5460u16, "192.168.80.4:2004".to_string()),
            (10922u16, "192.168.80.2:2004".to_string()),
            (16383u16, "192.168.80.3:2004".to_string()),
        ];

        assert_eq!(slots.nodes, nodes);
        assert_eq!(slots.masters.into_iter().collect::<Vec<_>>(), masters);
        assert_eq!(slots.replicas.into_iter().collect::<Vec<_>>(), replicas);
    }
}