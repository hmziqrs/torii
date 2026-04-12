use std::{pin::Pin, sync::Arc};

use anyhow::{Result, anyhow};
use bytes::Bytes;
use futures::{Stream, StreamExt as _, stream};
use tokio_util::io::ReaderStream;

use crate::{domain::request::BodyType, infra::blobs::BlobStore};

pub type PayloadStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

pub enum RequestBodyPayload {
    None,
    Bytes(Bytes),
    Stream(PayloadStream),
}

pub struct BuiltRequestBody {
    pub payload: RequestBodyPayload,
    pub content_type: Option<String>,
}

pub fn build_request_body_payload(
    body: &BodyType,
    blob_store: &Arc<BlobStore>,
) -> Result<BuiltRequestBody> {
    match body {
        BodyType::None => Ok(BuiltRequestBody {
            payload: RequestBodyPayload::None,
            content_type: None,
        }),
        BodyType::RawText { content } => Ok(BuiltRequestBody {
            payload: RequestBodyPayload::Bytes(Bytes::from(content.clone())),
            content_type: Some("text/plain".to_string()),
        }),
        BodyType::RawJson { content } => Ok(BuiltRequestBody {
            payload: RequestBodyPayload::Bytes(Bytes::from(content.clone())),
            content_type: Some("application/json".to_string()),
        }),
        BodyType::UrlEncoded { entries } => {
            let pairs: Vec<(String, String)> = entries
                .iter()
                .filter(|e| e.enabled)
                .map(|e| (e.key.clone(), e.value.clone()))
                .collect();
            let encoded = serde_urlencoded::to_string(&pairs)
                .map_err(|e| anyhow!("failed to encode url-form body: {e}"))?;
            Ok(BuiltRequestBody {
                payload: RequestBodyPayload::Bytes(Bytes::from(encoded)),
                content_type: Some("application/x-www-form-urlencoded".to_string()),
            })
        }
        BodyType::FormData {
            text_fields,
            file_fields,
        } => {
            let boundary = format!("torii-{}", uuid::Uuid::now_v7());
            let mut segments: Vec<PayloadStream> = Vec::new();

            for field in text_fields.iter().filter(|f| f.enabled) {
                segments.push(single_chunk(format!("--{}\r\n", boundary).into_bytes()));
                segments.push(single_chunk(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                        field.key
                    )
                    .into_bytes(),
                ));
                segments.push(single_chunk(field.value.clone().into_bytes()));
                segments.push(single_chunk(b"\r\n".to_vec()));
            }

            for field in file_fields.iter().filter(|f| f.enabled) {
                if field.blob_hash.trim().is_empty() {
                    return Err(anyhow!(
                        "form-data file '{}' has no selected file",
                        field.key
                    ));
                }
                segments.push(single_chunk(format!("--{}\r\n", boundary).into_bytes()));
                segments.push(single_chunk(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                        field.key,
                        field
                            .file_name
                            .clone()
                            .unwrap_or_else(|| "file.bin".to_string())
                    )
                    .into_bytes(),
                ));
                segments.push(single_chunk(
                    b"Content-Type: application/octet-stream\r\n\r\n".to_vec(),
                ));
                segments.push(blob_file_stream(blob_store, &field.blob_hash)?);
                segments.push(single_chunk(b"\r\n".to_vec()));
            }

            segments.push(single_chunk(format!("--{}--\r\n", boundary).into_bytes()));

            let chained = stream::iter(segments).flatten();
            Ok(BuiltRequestBody {
                payload: RequestBodyPayload::Stream(Box::pin(chained)),
                content_type: Some(format!("multipart/form-data; boundary={boundary}")),
            })
        }
        BodyType::BinaryFile {
            blob_hash,
            file_name,
        } => {
            if blob_hash.trim().is_empty() {
                return Err(anyhow!(
                    "binary body file is not selected{}",
                    file_name
                        .as_deref()
                        .map(|name| format!(" ({name})"))
                        .unwrap_or_default()
                ));
            }
            Ok(BuiltRequestBody {
                payload: RequestBodyPayload::Stream(blob_file_stream(blob_store, blob_hash)?),
                content_type: Some("application/octet-stream".to_string()),
            })
        }
    }
}

fn single_chunk(bytes: Vec<u8>) -> PayloadStream {
    Box::pin(stream::once(async move { Ok(Bytes::from(bytes)) }))
}

fn blob_file_stream(blob_store: &Arc<BlobStore>, blob_hash: &str) -> Result<PayloadStream> {
    let blob_hash_owned = blob_hash.to_string();
    let file = blob_store
        .open_read(blob_hash)
        .map_err(|e| anyhow!("failed to open body blob '{blob_hash}': {e}"))?;
    let file = tokio::fs::File::from_std(file);
    let stream = ReaderStream::new(file).map(move |item| {
        item.map_err(|e| anyhow!("failed to stream blob '{}': {e}", blob_hash_owned))
    });
    Ok(Box::pin(stream))
}
