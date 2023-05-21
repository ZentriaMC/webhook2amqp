#![allow(clippy::redundant_pattern_matching)]

use std::net::SocketAddr;
use std::sync::Arc;

use lapin::BasicProperties;
use lapin::{options::BasicPublishOptions, Connection};
use mlua::Lua;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;
use tracing::info;

mod lua;
mod lua_config;
mod request;
mod svc;
mod tokio_util;

type Error = Box<dyn std::error::Error + Send + Sync>;

pub struct Payload {
    request_id: String,
    qname: String,
    mime_type: String,
    body: Vec<u8>,
}

async fn ctrl_c() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install sigint handler")
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let amqp_addr = std::env::var("WEBHOOK2AMQP_AMQP_ADDR")
        .unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());

    let server_addr: SocketAddr = std::env::var("WEBHOOK2AMQP_LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;

    let lua_sandbox = std::env::var("WEBHOOK2AMQP_LUA_SANDBOX").unwrap_or_else(|_| "./lua".into());

    let config_file =
        std::env::var("WEBHOOK2AMQP_LUA_KV_CONFIG").unwrap_or_else(|_| "./config.jsonc".into());

    // Load configuration
    let lua_config = lua_config::load_config(config_file).await?;

    // Initialize Lua runtime
    let lua = Arc::new(Mutex::new(Lua::new()));
    let create_queue_names = {
        let lua = lua.lock().await;
        crate::lua::setup_engine(&lua, lua_sandbox, lua_config).await?
    };

    let amqp_conn = Arc::new(
        lapin::Connection::connect(
            &amqp_addr,
            lapin::ConnectionProperties::default().with_connection_name("webhook2amqp".into()),
        )
        .await?,
    );
    info!("amqp connection established");

    // Create a channel for passing payload from HTTP server to AMQP
    let (tx, rx) = tokio::sync::mpsc::channel::<Payload>(1);

    let local = tokio::task::LocalSet::new();

    local.spawn_local(amqp_task(amqp_conn.clone(), create_queue_names, rx));
    local.spawn_local(http_server_task(lua.clone(), server_addr, tx));
    local.await;

    amqp_conn.close(0, "").await?;
    info!("bye");

    Ok(())
}

async fn amqp_task(
    conn: Arc<Connection>,
    queues: Vec<String>,
    mut rx: Receiver<Payload>,
) -> Result<(), Error> {
    let channel = conn.create_channel().await?;

    // Create queues
    for ref qname in queues {
        let queue = channel
            .queue_declare(
                qname,
                lapin::options::QueueDeclareOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await?;

        info!("created queue '{}'", queue.name());
    }

    // Relay messages
    while let Some(received) = rx.recv().await {
        let opts = BasicPublishOptions::default();
        let props = BasicProperties::default()
            .with_content_type(received.mime_type.into())
            .with_message_id(received.request_id.into());
        let qname = &received.qname;

        channel
            .basic_publish("", qname, opts, &received.body, props)
            .await?
            .await?;

        info!("delivered message to queue '{qname}'");
    }

    channel.close(0, "").await?;
    Ok(())
}

async fn http_server_task(
    lua: Arc<Mutex<Lua>>,
    server_addr: SocketAddr,
    tx: Sender<Payload>,
) -> Result<(), Error> {
    let svc = svc::MakeSvc(lua.clone(), tx);

    hyper::Server::bind(&server_addr)
        .executor(tokio_util::LocalExec)
        .serve(svc)
        .with_graceful_shutdown(ctrl_c())
        .await?;

    Ok(())
}
