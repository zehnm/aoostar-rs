// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Prometheus client with Protocol Buffer support

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use aster_prom::{CONTENT_TYPE_PROTOBUF, CONTENT_TYPE_TEXT, ClientConfig, PromClient};
use clap::Parser;
use env_logger::Env;
use itertools::Itertools;
use log::{debug, error};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufWriter, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Duration;
use tempfile::Builder;
use url::Url;

/// A tool to scrape a Prometheus client and dump the result as sensor data for asterctl
/// (supports both text and protobuf formats).
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path or URL to metrics to convert, if omitted, defaults to read from STDIN
    #[arg(value_name = "METRICS_PATH | METRICS_URL")]
    input: Option<String>,
    /// Output sensor file.
    #[arg(short, long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Temporary directory for preparing the output sensor file.
    ///
    /// The system temp directory is used if not specified.
    /// The temp directory must be on the same file system for atomic rename operation!
    #[arg(short, long)]
    temp_dir: Option<PathBuf>,
    /// Print values in console
    #[arg(long)]
    console: bool,
    /// Client certificate file.
    #[arg(long, value_name = "FILE", requires("key"))]
    cert: Option<String>,
    /// Client certificate's key file.
    #[arg(long, value_name = "FILE", requires = "cert")]
    key: Option<String>,
    /// Accept any certificate during TLS handshake. Insecure, use only for testing.
    #[arg(long)]
    accept_invalid_cert: bool,
    /// The connect timeout in seconds for the HTTP request.
    #[arg(long, default_value_t = 10, value_name = "SECONDS")]
    connect_timeout: u8,
    /// The total timeout in seconds for the HTTP request.
    #[arg(long, default_value_t = 15, value_name = "SECONDS")]
    timeout: u8,
    /// Use Protocol Buffer format if available instead of text format. EXPERIMENTAL!
    #[arg(short, long)]
    proto: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    if let Some(out_file) = &args.out
        && let Some(parent) = out_file.parent()
    {
        fs::create_dir_all(parent)?;
    }

    let config = ClientConfig {
        cert_path: args.cert,
        key_path: args.key,
        accept_invalid_cert: args.accept_invalid_cert,
        connect_timeout: Duration::from_secs(args.connect_timeout as u64),
        timeout: Duration::from_secs(args.timeout as u64),
    };

    let client = PromClient::new(config)?;
    let input = args.input;

    let (data, content_type) = if let Some(input_arg) = input {
        // Check if input is a URL or file path
        if let Ok(url) = Url::parse(&input_arg) {
            // It's a URL, fetch from HTTP
            if args.proto {
                client.fetch_proto_metrics(url.as_str()).await?
            } else {
                client.fetch_text_metrics(url.as_str()).await?
            }
        } else {
            // It's a file path, read from file
            let bytes = std::fs::read(&input_arg)?;
            let content_type = if input_arg.ends_with(".pb") || input_arg.ends_with(".protobuf") {
                CONTENT_TYPE_PROTOBUF.to_string()
            } else {
                CONTENT_TYPE_TEXT.to_string()
            };
            (bytes, content_type)
        }
    } else {
        // Read from STDIN
        let mut buffer = Vec::new();
        io::stdin().read_to_end(&mut buffer)?;
        (buffer, CONTENT_TYPE_TEXT.to_string())
    };

    let sensors = client.parse_sensor_data(&data, &content_type)?;
    if let Some(out_file) = &args.out {
        write_sensor_file(out_file, args.temp_dir.as_deref(), &sensors)?;
    }

    if args.console {
        // pretty print console output with sorted keys
        for (label, value) in sensors.iter().sorted() {
            println!("{}: {}", label, value);
        }
        println!();
    }

    Ok(())
}

// TODO put in shared lib
fn write_sensor_file(
    out_file: &Path,
    temp_dir: Option<&Path>,
    sensors: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if out_file.is_dir() {
        error!("Output cannot be a directory: {}", out_file.display());
        exit(1);
    }

    // make sure our sensor file can be read by everyone
    let all_read_perm = fs::Permissions::from_mode(0o664);
    let tmp_file = if let Some(temp_path) = temp_dir {
        fs::create_dir_all(temp_path)?;

        debug!("Creating a new named temp file in {temp_path:?}");
        Builder::new()
            .permissions(all_read_perm)
            .tempfile_in(temp_path)?
    } else {
        debug!("Creating a new named temp file");
        Builder::new().permissions(all_read_perm).tempfile()?
    };

    debug!("Writing sensor temp file...");
    let mut stream = BufWriter::new(&tmp_file);

    for (label, value) in sensors.iter() {
        writeln!(stream, "{label}: {value}")?;
    }

    stream.flush()?;
    drop(stream);
    debug!("Renaming temp file to: {out_file:?}");
    tmp_file.persist(out_file)?;

    Ok(())
}
