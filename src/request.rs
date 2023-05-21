use std::collections::HashMap;
use std::net::SocketAddr;

use anyhow::{anyhow, Context};
use bytes::Buf;
use hyper::body::HttpBody;
use hyper::{Body, Request};
use mime::Mime;
use mlua::{ExternalResult, UserData};

const MAX_BODY_SIZE: usize = 1 << 27; // 128 MiB

#[derive(Clone)]
pub struct LuaRequest {
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub origin: String,
    pub headers: HashMap<String, String>,
    pub mime_type: Option<Mime>,
    pub body: Vec<u8>,
}

unsafe impl Send for LuaRequest {}
unsafe impl Sync for LuaRequest {}

impl LuaRequest {
    pub async fn create(request: Request<Body>, address: &SocketAddr) -> anyhow::Result<Self> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let url = request.uri().path().to_string();
        let method = request.method().to_string().to_ascii_uppercase();

        let mut headers: HashMap<String, String> = HashMap::new();
        for (name, value) in request.headers() {
            headers.insert(
                name.as_str().into(),
                String::from_utf8_lossy(value.as_bytes()).to_string(),
            );
        }

        let raw_mime_type = match headers.get("content-type") {
            Some(v) => Some(v.clone()),
            None => {
                if method != "GET" && method != "HEAD" {
                    Some("application/octet-stream".to_string())
                } else {
                    None
                }
            }
        };

        let mime_type = if let Some(v) = raw_mime_type {
            Some(v.parse::<Mime>()?)
        } else {
            None
        };

        let body = collect_body(request.into_body(), MAX_BODY_SIZE).await?;

        Ok(Self {
            request_id,
            url,
            method,
            origin: address.to_string(),
            headers,
            mime_type,
            body,
        })
    }
}

impl UserData for LuaRequest {
    fn add_fields<'lua, F: mlua::UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("request_id", |_, this| Ok(this.request_id.clone()));
        fields.add_field_method_get("url", |_, this| Ok(this.url.clone()));
        fields.add_field_method_get("method", |_, this| Ok(this.method.clone()));
        fields.add_field_method_get("origin", |_, this| Ok(this.origin.clone()));
        fields.add_field_method_get("mimetype", |_, this| {
            Ok(this.mime_type.as_ref().map(|v| v.to_string()))
        });
        fields.add_field_method_get("headers", |lua, this| {
            let headers = lua.create_table()?;
            for (key, value) in this.headers.iter() {
                headers.set(key.clone().to_lowercase(), value.clone().to_owned())?;
            }

            Ok(headers)
        });
        fields.add_field_method_get("body", |_, this| {
            String::from_utf8(this.body.clone())
                .context("body is not utf8")
                .to_lua_err()
        });
    }

    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(_methods: &mut M) {}
}

async fn collect_body<T>(body: T, max_len: usize) -> anyhow::Result<Vec<u8>>
where
    T: HttpBody,
{
    let size_hint = body.size_hint().lower() as usize;
    if size_hint > max_len {
        return Err(anyhow!("body too large"));
    }

    let mut current_size: usize = 0;
    let mut v = Vec::with_capacity(size_hint);

    futures_util::pin_mut!(body);
    while let Some(buf) = body.data().await {
        let mut buf = match buf {
            Ok(buf) => buf,
            // TODO: use err
            Err(_err) => return Err(anyhow!("failed to get buf")),
        };

        if buf.has_remaining() {
            let rem = buf.remaining();
            if current_size + rem > max_len {
                return Err(anyhow!("body too large"));
            }

            let mut copy = buf.copy_to_bytes(rem).to_vec();
            let len = copy.len();
            current_size += len;
            v.append(&mut copy);
        }
    }

    Ok(v)
}
