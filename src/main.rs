#![allow(clippy::redundant_pattern_matching)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
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

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// AMQP server URI
    #[arg(
        short = 'u',
        long,
        env = "WEBHOOK2AMQP_AMQP_URI",
        default_value = "amqp://127.0.0.1:5672/%2f"
    )]
    amqp_uri: String,

    /// HTTP server listen address
    #[arg(
        short = 'l',
        long,
        env = "WEBHOOK2AMQP_HTTP_LISTEN_ADDR",
        default_value = "127.0.0.1:3000"
    )]
    http_listen_addr: String,

    /// Lua scripts directory
    #[arg(
        short = 's',
        long,
        env = "WEBHOOK2AMQP_LUA_SANDBOX",
        default_value = "./lua"
    )]
    lua_sandbox: PathBuf,

    /// Lua scripts configuration (JSONC)
    #[arg(
        short = 'c',
        long,
        env = "WEBHOOK2AMQP_LUA_CONFIG",
        default_value = "./config.jsonc"
    )]
    lua_config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let server_addr = args.http_listen_addr.parse()?;

    // Load configuration
    let lua_config = lua_config::load_config(&args.lua_config).await?;

    // Initialize Lua runtime
    let lua = Arc::new(Mutex::new(Lua::new()));
    let create_queue_names = {
        let lua = lua.lock().await;
        crate::lua::setup_engine(&lua, &args.lua_sandbox, lua_config).await?
    };

    info!("connecting to amqp server");
    let amqp_conn = Arc::new(
        lapin::Connection::connect(
            &args.amqp_uri,
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
