use std::path::Path;

use anyhow::Context;
use mlua::{Function, Lua, Table, Value};

use crate::{lua_config::KVConfig, Error};

pub async fn setup_engine<P>(
    lua: &Lua,
    sandbox_path: P,
    config: KVConfig,
) -> Result<Vec<String>, Error>
where
    P: AsRef<Path>,
{
    let prepend_package_path: Function = lua
        .load(mlua::chunk! {
            function(path)
                package.path = path .. ";" .. package.path
            end
        })
        .eval()
        .context("failed to create package.path prepend function")?;

    let sandbox = sandbox_path.as_ref();
    let package_paths = &[
        sandbox.join("?.ljbc"),
        sandbox.join("?.lua"),
        sandbox.join("?/init.ljbc"),
        sandbox.join("?/init.lua"),
    ];

    let package_path = package_paths
        .clone()
        .map(|path| String::from(path.to_str().unwrap()))
        .join(";");

    prepend_package_path
        .call::<String, _>(package_path)
        .context("unable to adjust package.path")?;

    // Wire configuration
    lua.globals().set("CONFIG", config)?;

    // Overwrite print
    lua.globals().set(
        "print",
        lua.create_function(|_, args: mlua::MultiValue| {
            let mut str_args: Vec<String> = Vec::with_capacity(args.len());
            for arg in args {
                str_args.push(match arg {
                    Value::Nil => "nil".into(),
                    Value::Boolean(v) => format!("{}", v),
                    Value::Integer(v) => format!("{}", v),
                    Value::Number(v) => format!("{}", v),
                    Value::String(v) => format!("{}", v.to_string_lossy()),
                    Value::Table(v) => format!("(table: {:?})", v.to_pointer()),
                    Value::Function(v) => format!("(function: {:?})", v.info()),
                    Value::LightUserData(_) => "(LightUserData)".into(),
                    Value::Thread(_) => "(Thread)".into(),
                    Value::UserData(_) => "(UserData)".into(),
                    Value::Error(v) => format!("(error: {:?})", v),
                });
            }

            tracing::info!("{}", str_args.join(" "));

            Ok(())
        })?,
    )?;

    // Load the handler
    let handler: Table = lua
        .load(mlua::chunk! { require("mod") })
        .eval()
        .context("unable to load handler table")?;

    // Grab queue names
    let queue_names: Table = handler
        .get("queue_names")
        .context("no 'queue_names' in handler table")?;

    // Store request handler function
    let handler_func: Function = handler
        .get("handler")
        .context("no 'handler' in handler table")?;

    lua.set_named_registry_value("request_handler", handler_func)?;

    // Determine queues to create
    let mut create_queue_names = vec![];
    for pair in queue_names.pairs::<mlua::Integer, mlua::String>() {
        let (_, value) = pair?;
        create_queue_names.push(value.to_str()?.into());
    }

    Ok(create_queue_names)
}
