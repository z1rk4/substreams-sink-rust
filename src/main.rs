use anyhow::{format_err, Context, Error};
use futures03::StreamExt;
use lazy_static::lazy_static;
use pb::sf::substreams::rpc::v2::{BlockScopedData, BlockUndoSignal};
use pb::sf::substreams::v1::Package;
use redis::Commands;
use regex::Regex;
use semver::Version;

use prost::Message;
use serde::{Deserialize, Serialize};
use std::{process::exit, sync::Arc};
use substreams::SubstreamsEndpoint;
use substreams_stream::{BlockResponse, SubstreamsStream};
use warp::Filter;

mod pb;
mod substreams;
mod substreams_stream;

lazy_static! {
    static ref MODULE_NAME_REGEXP: Regex = Regex::new(r"^([a-zA-Z][a-zA-Z0-9_-]{0,63})$").unwrap();
}

const REGISTRY_URL: &str = "https://spkg.io";

#[derive(Serialize, Deserialize, Debug)]
pub struct EosSimpleBlock {
    pub head_block_id: String,
    pub head_block_number: u64,
    pub head_block_time: String,
}

enum RedisConnection {
    Cluster(redis::cluster::ClusterConnection),
    Single(redis::Connection),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv()?;

    let mut endpoint_url = dotenvy::var("SUBSTREAMS_ENDPOINT_URL").unwrap();
    let package_file = dotenvy::var("SUBSTREAMS_PACKAGE_FILE").unwrap();
    let module_name = dotenvy::var("SUBSTREAMS_MODULE").unwrap();

    if !endpoint_url.starts_with("http") {
        endpoint_url = format!("{}://{}", "https", endpoint_url);
    }

    let token_env = dotenvy::var("SUBSTREAMS_API_TOKEN").unwrap_or("".to_string());
    let mut token: Option<String> = None;
    if token_env.len() > 0 {
        token = Some(token_env);
    }

    let package = read_package(&package_file).await?;
    let block_range = read_block_range(&package, &module_name)?;
    let endpoint = Arc::new(SubstreamsEndpoint::new(&endpoint_url, token).await?);

    /* Set up a server for monitoring */
    let is_healthy_route = warp::path("isHealthy")
        .and(warp::get())
        .map(|| warp::reply::json(&true));

    tokio::spawn(async move {
        warp::serve(is_healthy_route)
            .run(([127, 0, 0, 1], 3030))
            .await
    });

    /* Custom redis handling */
    let redis_hosts: Vec<String> = dotenvy::var("REDIS_HOST")?
        .split(",")
        .map(|s| s.to_string())
        .collect();
    let mut redis_connection = if redis_hosts.len() > 1 {
        // TODO: change to hosts len > 0
        let redis_client = redis::cluster::ClusterClientBuilder::new(redis_hosts)
            .retries(100)
            .min_retry_wait(3000)
            .build()
            .unwrap();
        RedisConnection::Cluster(redis_client.get_connection().unwrap())
    } else {
        let redis_client = redis::Client::open(redis_hosts.get(0).unwrap().clone()).unwrap();
        RedisConnection::Single(redis_client.get_connection().unwrap())
    };

    let cursor: Option<String> = load_persisted_cursor(&mut redis_connection)?;

    let mut stream = SubstreamsStream::new(
        endpoint,
        cursor,
        package.modules,
        module_name.to_string(),
        block_range.0,
        block_range.1,
    );

    loop {
        match stream.next().await {
            None => {
                println!("Stream consumed");
                break;
            }
            Some(Ok(BlockResponse::New(data))) => {
                process_block_scoped_data(&mut redis_connection, &data)?;
                persist_cursor(&mut redis_connection, data.cursor)?;
            }
            Some(Ok(BlockResponse::Undo(undo_signal))) => {
                process_block_undo_signal(&undo_signal)?;
                persist_cursor(&mut redis_connection, undo_signal.last_valid_cursor)?;
            }
            Some(Err(err)) => {
                println!();
                println!("Stream terminated with error");
                println!("{:?}", err);
                exit(1);
            }
        }
    }

    Ok(())
}

fn process_block_scoped_data(
    connection: &mut RedisConnection,
    data: &BlockScopedData,
) -> Result<(), Error> {
    // let output = data.output.as_ref().unwrap().map_output.as_ref().unwrap();

    // You can decode the actual Any type received using this code:
    //
    //     let value = GeneratedStructName::decode(output.value.as_slice())?;
    //
    // Where GeneratedStructName is the Rust code generated for the Protobuf representing
    // your type, so you will need generate it using `substreams protogen` and import it from the
    // `src/pb` folder.

    let clock = data.clock.as_ref().unwrap();
    let timestamp = clock.timestamp.as_ref().unwrap();
    // let date = DateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
    //     .expect("received timestamp should always be valid");

    let block_number = clock.number;
    let block_hash = &clock.id;
    let block_key: String = format!("eos:simple:{}", block_number);

    let block_info = EosSimpleBlock {
        head_block_id: block_hash.clone(),
        head_block_number: block_number,
        head_block_time: timestamp.to_string(),
    };
    let block_info_json_string = serde_json::to_string(&block_info).unwrap();

    match connection {
        RedisConnection::Cluster(cluster_conn) => {
            let _: () = cluster_conn
                .set_ex(block_key, block_info_json_string, 15)
                .unwrap(); // TODO: change to 15 mins TTL
        }
        RedisConnection::Single(single_conn) => {
            let _: () = single_conn
                .set_ex(block_key, block_info_json_string, 15)
                .unwrap(); // TODO: change to 15 mins TTL
        }
    }

    println!("Block {}: {:}", block_number, block_hash);

    // println!(
    //     "Block #{} - Payload {} ({} bytes) - Drift {}s",
    //     clock.number,
    //     output.type_url.replace("type.googleapis.com/", ""),
    //     output.value.len(),
    //     date.signed_duration_since(chrono::offset::Utc::now())
    //         .num_seconds()
    //         * -1
    // );

    Ok(())
}

fn process_block_undo_signal(_undo_signal: &BlockUndoSignal) -> Result<(), anyhow::Error> {
    // `BlockUndoSignal` must be treated as "delete every data that has been recorded after
    // block height specified by block in BlockUndoSignal". In the example above, this means
    // you must delete changes done by `Block #7b` and `Block #6b`. The exact details depends
    // on your own logic. If for example all your added record contain a block number, a
    // simple way is to do `delete all records where block_num > 5` which is the block num
    // received in the `BlockUndoSignal` (this is true for append only records, so when only `INSERT` are allowed).
    unimplemented!("you must implement some kind of block undo handling, or request only final blocks (tweak substreams_stream.rs)")
}

fn persist_cursor(connection: &mut RedisConnection, cursor: String) -> Result<(), anyhow::Error> {
    // FIXME: Handling of the cursor is missing here. It should be saved each time
    // a full block has been correctly processed/persisted. The saving location
    // is your responsibility.
    //
    // By making it persistent, we ensure that if we crash, on startup we are
    // going to read it back from database and start back our SubstreamsStream
    // with it ensuring we are continuously streaming without ever losing a single
    // element.
    match connection {
        RedisConnection::Cluster(cluster_conn) => {
            cluster_conn.set_ex("lootbox:eos:cursor", cursor, 7 * 24 * 60 * 60)?;
            // 7 days persistence
        }
        RedisConnection::Single(single_conn) => {
            single_conn.set_ex("lootbox:eos:cursor", cursor, 7 * 24 * 60 * 60)?;
            // 7 days persistence
        }
    }
    Ok(())
}

fn load_persisted_cursor(
    connection: &mut RedisConnection,
) -> Result<Option<String>, anyhow::Error> {
    // FIXME: Handling of the cursor is missing here. It should be loaded from
    // somewhere (local file, database, cloud storage) and then `SubstreamStream` will
    // be able correctly resume from the right block.
    match connection {
        RedisConnection::Cluster(cluster_conn) => cluster_conn
            .get("lootbox:eos:cursor")
            .map_err(|e| anyhow::anyhow!("{}", e)),
        RedisConnection::Single(single_conn) => single_conn
            .get("lootbox:eos:cursor")
            .map_err(|e| anyhow::anyhow!("{}", e)),
    }
}

fn read_block_range(pkg: &Package, module_name: &str) -> Result<(i64, u64), anyhow::Error> {
    let module = pkg
        .modules
        .as_ref()
        .unwrap()
        .modules
        .iter()
        .find(|m| m.name == module_name)
        .ok_or_else(|| format_err!("module '{}' not found in package", module_name))?;

    let input: String = dotenvy::var("SUBSTREAMS_BLOCK_RANGE").unwrap_or("".to_string());

    let (prefix, suffix) = match input.split_once(":") {
        Some((prefix, suffix)) => (prefix.to_string(), suffix.to_string()),
        None => ("".to_string(), input),
    };

    let start: i64 = match prefix.as_str() {
        "" => module.initial_block as i64,
        x if x.starts_with("+") => {
            let block_count = x
                .trim_start_matches("+")
                .parse::<u64>()
                .context("argument <stop> is not a valid integer")?;

            (module.initial_block + block_count) as i64
        }
        x => x
            .parse::<i64>()
            .context("argument <start> is not a valid integer")?,
    };

    let stop: u64 = match suffix.as_str() {
        "" => 0,
        "-" => 0,
        x if x.starts_with("+") => {
            let block_count = x
                .trim_start_matches("+")
                .parse::<u64>()
                .context("argument <stop> is not a valid integer")?;

            start as u64 + block_count
        }
        x => x
            .parse::<u64>()
            .context("argument <stop> is not a valid integer")?,
    };

    return Ok((start, stop));
}

async fn read_package(input: &str) -> Result<Package, anyhow::Error> {
    let mut mutable_input = input.to_string();

    let val = parse_standard_package_and_version(input);
    if val.is_ok() {
        let package_and_version = val.unwrap();
        mutable_input = format!(
            "{}/v1/packages/{}/{}",
            REGISTRY_URL, package_and_version.0, package_and_version.1
        );
    }

    if mutable_input.starts_with("http") {
        return read_http_package(&mutable_input).await;
    }

    // Assume it's a local file
    let content = std::fs::read(&mutable_input)
        .context(format_err!("read package from file '{}'", mutable_input))?;
    Package::decode(content.as_ref()).context("decode command")
}
async fn read_http_package(input: &str) -> Result<Package, anyhow::Error> {
    let body = reqwest::get(input).await?.bytes().await?;

    Package::decode(body).context("decode command")
}

fn parse_standard_package_and_version(input: &str) -> Result<(String, String), Error> {
    let parts: Vec<&str> = input.split('@').collect();
    if parts.len() > 2 {
        return Err(format_err!(
            "package name: {} does not follow the convention of <package>@<version>",
            input
        ));
    }

    let package_name = parts[0].to_string();
    if !MODULE_NAME_REGEXP.is_match(&package_name) {
        return Err(format_err!(
            "package name {} does not match regexp {}",
            package_name,
            MODULE_NAME_REGEXP.as_str()
        ));
    }

    if parts.len() == 1
        || parts
            .get(1)
            .map_or(true, |v| v.is_empty() || *v == "latest")
    {
        return Ok((package_name, "latest".to_string()));
    }

    let version = parts[1];
    if !is_valid_version(&version.replace("v", "")) {
        return Err(format_err!(
            "version '{}' is not valid Semver format",
            version
        ));
    }

    Ok((package_name, version.to_string()))
}

fn is_valid_version(version: &str) -> bool {
    Version::parse(version).is_ok()
}
