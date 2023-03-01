use crate::codec::kafka::RequestHeader as CodecRequestHeader;
use anyhow::{anyhow, Context, Result};
use bytes::{BufMut, Bytes, BytesMut};
use kafka_protocol::messages::{
    ApiKey, FindCoordinatorRequest, FindCoordinatorResponse, ProduceRequest, ProduceResponse,
    RequestHeader, ResponseHeader,
};
use kafka_protocol::protocol::{Decodable, Encodable};

// No way to know which version to use, just have to guess
const REQUEST_HEADER_VERSION: i16 = 1;
const RESPONSE_HEADER_VERSION: i16 = 0;

#[derive(Debug, PartialEq, Clone)]
pub enum KafkaFrame {
    Request {
        header: RequestHeader,
        body: RequestBody,
    },
    Response {
        version: i16,
        header: ResponseHeader,
        body: ResponseBody,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub enum RequestBody {
    Produce(ProduceRequest),
    FindCoordinator(FindCoordinatorRequest),
    Unknown { api_key: ApiKey, message: Bytes },
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseBody {
    Produce(ProduceResponse),
    FindCoordinator(FindCoordinatorResponse),
    Unknown { api_key: ApiKey, message: Bytes },
}

impl KafkaFrame {
    pub fn from_bytes(
        mut bytes: Bytes,
        request_header: Option<CodecRequestHeader>,
    ) -> Result<Self> {
        // remove length header
        let _ = bytes.split_to(4);

        match request_header {
            Some(request_header) => KafkaFrame::parse_response(bytes, request_header),
            None => KafkaFrame::parse_request(bytes),
        }
    }

    fn parse_request(mut bytes: Bytes) -> Result<Self> {
        let header = RequestHeader::decode(&mut bytes, REQUEST_HEADER_VERSION)
            .context("Failed to decode request header")?;

        let api_key = ApiKey::try_from(header.request_api_key)
            .map_err(|_| anyhow!("unknown api key {}", header.request_api_key))?;
        let version = header.request_api_version;
        let body = match api_key {
            ApiKey::ProduceKey => RequestBody::Produce(decode(&mut bytes, version)?),
            ApiKey::FindCoordinatorKey => {
                RequestBody::FindCoordinator(decode(&mut bytes, version)?)
            }
            api_key => RequestBody::Unknown {
                api_key,
                message: bytes,
            },
        };

        Ok(KafkaFrame::Request { header, body })
    }

    fn parse_response(mut bytes: Bytes, request_header: CodecRequestHeader) -> Result<Self> {
        let header = ResponseHeader::decode(&mut bytes, RESPONSE_HEADER_VERSION)
            .context("Failed to decode response header")?;

        let version = request_header.version;
        let body = match request_header.api_key {
            ApiKey::ProduceKey => ResponseBody::Produce(decode(&mut bytes, version)?),
            ApiKey::FindCoordinatorKey => {
                ResponseBody::FindCoordinator(decode(&mut bytes, version)?)
            }
            api_key => ResponseBody::Unknown {
                api_key,
                message: bytes,
            },
        };

        Ok(KafkaFrame::Response {
            version,
            header,
            body,
        })
    }

    pub fn encode(self, bytes: &mut BytesMut) -> Result<()> {
        // write dummy length
        let length_start = bytes.len();
        let bytes_start = length_start + 4;
        bytes.put_i32(0);

        // write message
        match self {
            KafkaFrame::Request { header, body } => {
                header.encode(bytes, REQUEST_HEADER_VERSION)?;
                let version = header.request_api_version;
                match body {
                    RequestBody::Produce(x) => x.encode(bytes, version)?,
                    RequestBody::FindCoordinator(x) => x.encode(bytes, version)?,
                    RequestBody::Unknown { message, .. } => bytes.extend_from_slice(&message),
                }
            }
            KafkaFrame::Response {
                version,
                header,
                body,
            } => {
                header.encode(bytes, RESPONSE_HEADER_VERSION)?;
                match body {
                    ResponseBody::Produce(x) => x.encode(bytes, version)?,
                    ResponseBody::FindCoordinator(x) => x.encode(bytes, version)?,
                    ResponseBody::Unknown { message, .. } => bytes.extend_from_slice(&message),
                }
            }
        }

        // overwrite dummy length with actual length of serialized bytes
        let bytes_len = bytes.len() - bytes_start;
        bytes[length_start..bytes_start].copy_from_slice(&(bytes_len as i32).to_be_bytes());

        Ok(())
    }
}

fn decode<T: Decodable>(bytes: &mut Bytes, version: i16) -> Result<T> {
    T::decode(bytes, version).context(format!(
        "Failed to decode {} v{} body",
        std::any::type_name::<T>(),
        version
    ))
}