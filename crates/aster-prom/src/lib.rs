// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

mod prom_client;

// Include generated protobuf code
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/io.prometheus.client.rs"));
}

pub use prom_client::{ClientConfig, PromClient};

pub const CONTENT_TYPE_PROTOBUF: &str = "application/vnd.google.protobuf";
pub const CONTENT_TYPE_TEXT: &str = "text/plain";

pub const ACCEPT_HEADER_PROTOBUF: &str = "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited;q=0.7,text/plain;version=0.0.4;q=0.3";
pub const ACCEPT_HEADER_TEXT: &str = "text/plain;version=0.0.4;q=0.3";
