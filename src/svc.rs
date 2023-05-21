use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{net::SocketAddr, sync::Arc};

use futures_util::future;
use hyper::{server::conn::AddrStream, service::Service};
use hyper::{Body, Request, Response};
use mlua::{Function, Lua};
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::request::LuaRequest;
use crate::Payload;

pub struct WebhookService {
    lua: Arc<Mutex<Lua>>,
    remote_addr: SocketAddr,
    sender: Sender<Payload>,
}

impl Service<Request<Body>> for WebhookService {
    type Response = Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    // TODO
    // type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let lua = Arc::clone(&self.lua);
        let remote_addr = self.remote_addr;
        let sender = self.sender.clone();

        Box::pin(async move {
            let lua = lua.lock().await;

            let luareq = LuaRequest::create(req, &remote_addr).await?;
            debug!(
                "{} {} '{}' ({}) - {}",
                luareq.method,
                luareq.url,
                luareq
                    .mime_type
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or("(absent)".to_string()),
                luareq.request_id,
                luareq.origin,
            );

            let handler: Function = lua.named_registry_value("request_handler")?;

            let response = Response::builder()
                .header("x-request-id", &luareq.request_id)
                .header("content-type", "text/plain; charset=utf-8");

            // TODO: pass in a reference, don't move
            match handler
                .call_async::<_, Option<String>>(luareq.clone())
                .await
            {
                Ok(Some(qname)) => {
                    debug!("routing webhook message to queue '{}'", qname);
                    if let Err(_) = sender
                        .send(Payload {
                            request_id: luareq.request_id,
                            qname,
                            mime_type: luareq
                                .mime_type
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or("application/octet-stream".to_string()),
                            body: luareq.body,
                        })
                        .await
                    {
                        error!("channel closed");
                        return Ok(response.status(503).body(Body::from("FAIL"))?);
                    }
                }
                Ok(None) => {
                    return Ok(response.status(400).body(Body::from("FAIL"))?);
                }
                Err(err) => {
                    error!(?err, "lua request handler failed");
                    return Ok(response.status(500).body(Body::from("ERROR"))?);
                }
            }

            Ok(response.status(200).body(Body::from("OK"))?)
        })
    }
}

pub struct MakeSvc(pub Arc<Mutex<Lua>>, pub Sender<Payload>);

impl Service<&AddrStream> for MakeSvc {
    type Response = WebhookService;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, stream: &AddrStream) -> Self::Future {
        let lua = self.0.clone();
        let sender = self.1.clone();
        let remote_addr = stream.remote_addr();
        Box::pin(future::ok(WebhookService {
            lua,
            remote_addr,
            sender,
        }))
    }
}
