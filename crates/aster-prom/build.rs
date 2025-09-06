//! Build script for generating Rust code from Prometheus protobuf definitions
//!
//! This build script generates Rust code using prost.

use std::io::Result;

fn main() -> Result<()> {
    // Configure prost-build
    let mut config = prost_build::Config::new();

    // Enable optional and required field distinction
    config.protoc_arg("--experimental_allow_proto3_optional");

    // Compile the Prometheus client_model protobuf files
    // The official protobuf definition is hosted on GitHub
    // https://github.com/prometheus/client_model/blob/master/io/prometheus/client/metrics.proto
    let proto_files = &["io/prometheus/client/metrics.proto"];
    let includes = &["proto"];

    // Build the protobuf files
    config.compile_protos(proto_files, includes)?;

    // Tell Cargo to rerun this build script if the proto file changes
    println!("cargo:rerun-if-changed=proto/io/prometheus/client/metrics.proto");

    Ok(())
}
