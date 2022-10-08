use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use bytes::Buf;
use futures::{SinkExt, StreamExt};
use hyper::body::HttpBody;
use hyper::{Body, Method, Request, Response, StatusCode};
use lapin::options::BasicPublishOptions;
use lapin::BasicProperties;
use mime::Mime;
use tracing::info;

const MAX_BODY_SIZE: usize = 1 << 27; // 128 MiB

type Err = Box<dyn std::error::Error + Sync + Send>;

struct Payload {
    mime_type: String,
    body: Vec<u8>,
}

async fn ctrl_c() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install sigint handler")
}

#[tokio::main]
async fn main() -> Result<(), Err> {
    tracing_subscriber::fmt::init();

    let amqp_addr = std::env::var("WEBHOOK2AMQP_AMQP_ADDR")
        .unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());

    let amqp_queue_name =
        std::env::var("WEBHOOK2AMQP_AMQP_QUEUE_NAME").unwrap_or_else(|_| "webhook".into());

    let server_addr: SocketAddr = std::env::var("WEBHOOK2AMQP_LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;

    let amqp_conn = Arc::new(
        lapin::Connection::connect(
            &amqp_addr,
            lapin::ConnectionProperties::default().with_connection_name("webhook2amqp".into()),
        )
        .await?,
    );
    info!("amqp connection established");

    // Create a channel for passing payload from HTTP server to AMQP
    let (tx, rx) = futures::channel::mpsc::channel::<Payload>(1);

    // Launch HTTP server
    let server = tokio::task::spawn(async move {
        let server =
            hyper::Server::bind(&server_addr).serve(hyper::service::make_service_fn(|_conn| {
                let tx = tx.clone();
                async {
                    Ok::<_, Err>(hyper::service::service_fn(move |req| {
                        // TODO: log errors
                        http_handler(req, tx.clone())
                    }))
                }
            }));

        server.with_graceful_shutdown(ctrl_c()).await?;

        Ok::<(), Err>(())
    });

    // Launch AMQP client
    let amqp_conn2 = amqp_conn.clone(); // XXX
    let amqp_client = tokio::task::spawn(async move {
        let mut rx = rx;
        let channel = amqp_conn2.create_channel().await?;

        let queue = channel
            .queue_declare(
                &amqp_queue_name,
                lapin::options::QueueDeclareOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await?;

        info!(?queue, "created queue {amqp_queue_name}");
        let qname = queue.name().as_str();

        while let Some(received) = rx.next().await {
            let opts = BasicPublishOptions::default();
            let props = BasicProperties::default().with_content_type(received.mime_type.into());

            channel
                .basic_publish("", qname, opts, &received.body, props)
                .await?
                .await?;

            info!("message delivered");
        }

        channel.close(0, "").await?;

        Ok::<(), Err>(())
    });

    futures::future::try_join_all(vec![server, amqp_client]).await?;
    amqp_conn.close(0, "").await?;
    info!("bye");

    Ok(())
}

async fn http_handler(
    req: Request<Body>,
    mut tx: futures::channel::mpsc::Sender<Payload>,
) -> Result<Response<Body>, Err> {
    let mut resp = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::POST, "/handle") => {
            // Grab mime type
            let raw_mime = if let Some(hdr) = req.headers().get("content-type") {
                hdr.to_str()
                    .context("expected content-type header to be valid string")?
            } else {
                "application/octet-stream"
            };
            let mime: Mime = raw_mime.parse()?;

            // Grab the body
            let body = collect_body(req.into_body(), MAX_BODY_SIZE).await?;

            tx.send(Payload {
                mime_type: mime.to_string(),
                body,
            })
            .await?;

            *resp.body_mut() = Body::from("OK");
        }
        _ => {
            *resp.status_mut() = StatusCode::NOT_FOUND;
        }
    }

    Ok(resp)
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
