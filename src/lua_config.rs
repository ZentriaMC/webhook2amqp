use std::fs::File;
use std::io::Read;
use std::{collections::HashMap, path::Path};

use anyhow::anyhow;
use serde::Deserialize;

use crate::Error;

pub type KVConfig = HashMap<String, String>;

pub async fn load_config<P: AsRef<Path>>(config_path: P) -> Result<KVConfig, Error> {
    let mut file = File::open(config_path)?;
    // let config: KVConfig = serde_json::from_reader(file)?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    let parsed = jsonc_parser::parse_to_serde_value(
        &buf,
        &jsonc_parser::ParseOptions {
            allow_comments: true,
            allow_loose_object_property_names: false,
            allow_trailing_commas: true,
        },
    )?;
    if parsed.is_none() {
        return Err(anyhow!("no value").into());
    }

    let config = KVConfig::deserialize(parsed.unwrap())?;
    Ok(config)
}
