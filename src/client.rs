use anyhow::{Result, anyhow};
use bytes::Buf;
use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor, MethodDescriptor};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};

pub struct AdsClient {
    channel: Channel,
    developer_token: String,
    login_customer_id: Option<String>,
    access_token: String,
    timeout: Option<std::time::Duration>,
}

impl AdsClient {
    pub async fn connect(
        endpoint: &str,
        developer_token: String,
        login_customer_id: Option<String>,
        access_token: String,
        timeout: Option<std::time::Duration>,
    ) -> Result<Self> {
        let endpoint = normalize_endpoint(endpoint)?;
        let mut builder = Endpoint::from_shared(endpoint)?;
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        let channel = builder.connect().await?;
        Ok(Self {
            channel,
            developer_token,
            login_customer_id,
            access_token,
            timeout,
        })
    }

    pub async fn unary(&self, method: &MethodDescriptor, message: DynamicMessage) -> Result<DynamicMessage> {
        let path = method_path(method);
        let codec = DynamicCodec::new(method.input(), method.output());
        let mut grpc = tonic::client::Grpc::new(self.channel.clone());
        let request = self.build_request(message)?;
        let response = grpc.unary(request, path, codec).await?;
        Ok(response.into_inner())
    }

    pub async fn server_stream(&self, method: &MethodDescriptor, message: DynamicMessage) -> Result<Vec<DynamicMessage>> {
        let path = method_path(method);
        let codec = DynamicCodec::new(method.input(), method.output());
        let mut grpc = tonic::client::Grpc::new(self.channel.clone());
        let request = self.build_request(message)?;
        let mut stream = grpc.server_streaming(request, path, codec).await?.into_inner();

        let mut out = Vec::new();
        while let Some(msg) = stream.message().await? {
            out.push(msg);
        }
        Ok(out)
    }

    pub async fn server_streaming_raw(
        &self,
        method: &MethodDescriptor,
        message: DynamicMessage,
    ) -> Result<tonic::Streaming<DynamicMessage>> {
        let path = method_path(method);
        let codec = DynamicCodec::new(method.input(), method.output());
        let mut grpc = tonic::client::Grpc::new(self.channel.clone());
        let request = self.build_request(message)?;
        let response = grpc.server_streaming(request, path, codec).await?;
        Ok(response.into_inner())
    }

    fn build_request<T>(&self, message: T) -> Result<Request<T>> {
        let mut request = Request::new(message);
        let metadata = request.metadata_mut();

        let developer = MetadataValue::try_from(self.developer_token.as_str())
            .map_err(|_| anyhow!("invalid developer token"))?;
        metadata.insert("developer-token", developer);

        let auth_value = format!("Bearer {}", self.access_token.trim());
        let auth = MetadataValue::try_from(auth_value.as_str())
            .map_err(|_| anyhow!("invalid access token"))?;
        metadata.insert("authorization", auth);

        if let Some(login_customer_id) = self.login_customer_id.as_ref() {
            let login = MetadataValue::try_from(login_customer_id.as_str())
                .map_err(|_| anyhow!("invalid login-customer-id"))?;
            metadata.insert("login-customer-id", login);
        }

        if let Some(timeout) = self.timeout {
            request.set_timeout(timeout);
        }

        Ok(request)
    }
}

fn normalize_endpoint(endpoint: &str) -> Result<String> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    Ok(format!("https://{}", endpoint))
}

fn method_path(method: &MethodDescriptor) -> tonic::codegen::http::uri::PathAndQuery {
    let service = method.parent_service().full_name();
    let path = format!("/{}/{}", service, method.name());
    path.parse().expect("valid grpc path")
}

struct DynamicCodec {
    output: MessageDescriptor,
}

impl DynamicCodec {
    fn new(_input: MessageDescriptor, output: MessageDescriptor) -> Self {
        Self { output }
    }
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            output: self.output.clone(),
        }
    }
}

struct DynamicEncoder;

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|err| Status::internal(format!("encode error: {err}")))
    }
}

struct DynamicDecoder {
    output: MessageDescriptor,
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        if src.remaining() == 0 {
            return Ok(None);
        }
        let bytes = src.copy_to_bytes(src.remaining());
        let msg = DynamicMessage::decode(self.output.clone(), bytes)
            .map_err(|err| Status::internal(format!("decode error: {err}")))?;
        Ok(Some(msg))
    }
}
