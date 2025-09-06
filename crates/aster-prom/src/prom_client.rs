// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

use crate::proto::{Metric as ProtoMetric, MetricFamily as ProtoMetricFamily, MetricType};
use crate::{ACCEPT_HEADER_PROTOBUF, ACCEPT_HEADER_TEXT, CONTENT_TYPE_PROTOBUF, CONTENT_TYPE_TEXT};
use itertools::Itertools;
use log::warn;
use prost::Message;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

/// HTTP client configuration
#[derive(Debug)]
pub struct ClientConfig {
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub accept_invalid_cert: bool,
    pub connect_timeout: Duration,
    pub timeout: Duration,
}

/// Main application structure
pub struct PromClient {
    client: reqwest::Client,
}

impl PromClient {
    /// Creates a new PromClient instance with the given configuration
    pub fn new(config: ClientConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let mut client_builder = reqwest::Client::builder()
            .danger_accept_invalid_certs(config.accept_invalid_cert)
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout);

        // Handle TLS client authentication if certificates are provided
        if let (Some(cert_path), Some(key_path)) = (config.cert_path, config.key_path) {
            let cert = std::fs::read(&cert_path)?;
            let key = std::fs::read(&key_path)?;

            let identity = reqwest::Identity::from_pem(&[cert, key].concat())?;
            client_builder = client_builder.identity(identity);
        }

        let client = client_builder.build()?;

        Ok(PromClient { client })
    }

    pub async fn fetch_text_metrics(
        &self,
        url: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        self.fetch_metrics(url, ACCEPT_HEADER_TEXT).await
    }

    pub async fn fetch_proto_metrics(
        &self,
        url: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        self.fetch_metrics(url, ACCEPT_HEADER_PROTOBUF).await
    }

    /// Fetches metrics from a URL and returns the response bytes and content type
    pub async fn fetch_metrics(
        &self,
        url: &str,
        accept_header: &str,
    ) -> Result<(Vec<u8>, String), Box<dyn std::error::Error>> {
        let response = self
            .client
            .get(url)
            .header("Accept", accept_header)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("HTTP request failed with status: {}", response.status()).into());
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|ct| ct.to_str().ok())
            .unwrap_or(CONTENT_TYPE_TEXT)
            .to_lowercase();

        let bytes = response.bytes().await?.to_vec();
        Ok((bytes, content_type))
    }

    /// Determines if the content is protobuf format based on content type
    pub fn is_protobuf_format(content_type: &str) -> bool {
        content_type.contains("protobuf") || content_type.contains(CONTENT_TYPE_PROTOBUF)
    }

    /// Parses protobuf-encoded metrics and converts to sensor hash map
    pub fn parse_protobuf_sensors(
        &self,
        data: &[u8],
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut sensors = HashMap::new();
        let mut cursor = 0;

        // Protobuf metrics are length-delimited messages
        while cursor < data.len() {
            // Read varint length
            let (length, varint_len) = self.read_varint(&data[cursor..])?;
            cursor += varint_len;

            if cursor + length > data.len() {
                break; // Invalid message
            }

            // Parse the MetricFamily message
            let message_data = &data[cursor..cursor + length];
            let proto_family = ProtoMetricFamily::decode(message_data)?;

            self.convert_proto_family_to_sensors(proto_family, &mut sensors)?;

            cursor += length;
        }

        Ok(sensors)
    }

    /// Read a varint from the beginning of a byte slice
    fn read_varint(&self, data: &[u8]) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let mut result = 0;
        let mut shift = 0;
        let mut bytes_read = 0;

        for &byte in data {
            bytes_read += 1;
            result |= ((byte & 0x7F) as usize) << shift;

            if byte & 0x80 == 0 {
                return Ok((result, bytes_read));
            }

            shift += 7;
            if shift >= 64 {
                return Err("Varint too long".into());
            }
        }

        Err("Incomplete varint".into())
    }

    /// Convert protobuf MetricFamily to simple key value pairs, similar as the text format.
    fn convert_proto_family_to_sensors(
        &self,
        family: ProtoMetricFamily,
        sensors: &mut HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metric_type = MetricType::try_from(family.r#type.unwrap_or(0))?;

        let label_name = family.name.as_deref().unwrap_or_default();
        for proto_metric in family.metric {
            match self.convert_proto_metric_to_sensors(&proto_metric, metric_type) {
                Ok((labels, value)) => {
                    let key = if labels.is_empty() {
                        label_name.to_string()
                    } else {
                        format!("{label_name}{{{labels}}}",)
                    };
                    // colon character is not allowed in key since it is used as a separator
                    sensors.insert(key.replace(':', "_"), value);
                }
                Err(e) => {
                    warn!(
                        "Failed to convert metric {}: {e}",
                        family.name.as_deref().unwrap_or_default()
                    );
                }
            }
        }

        Ok(())
    }

    /// Convert protobuf Metric to text format
    fn convert_proto_metric_to_sensors(
        &self,
        proto_metric: &ProtoMetric,
        metric_type: MetricType,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        // Convert labels
        let labels = proto_metric
            .label
            .iter()
            .filter_map(|pair| {
                if let (Some(name), Some(value)) = (&pair.name, &pair.value) {
                    Some(format!("{}=\"{}\"", name, value))
                } else {
                    None
                }
            })
            .join(",");

        match metric_type {
            MetricType::Counter => {
                if let Some(counter) = &proto_metric.counter {
                    Ok((labels, counter.value.unwrap_or(0.0).to_string()))
                } else {
                    Err("Counter metric missing counter field".into())
                }
            }
            MetricType::Gauge => {
                if let Some(gauge) = &proto_metric.gauge {
                    Ok((labels, gauge.value.unwrap_or(0.0).to_string()))
                } else {
                    Err("Gauge metric missing gauge field".into())
                }
            }
            MetricType::Summary => {
                if let Some(_summary) = &proto_metric.summary {
                    // let mut quantiles = HashMap::new();
                    // for quantile in &summary.quantile {
                    //     if let (Some(q), Some(v)) = (quantile.quantile, quantile.value) {
                    //         quantiles.insert(q.to_string(), v.to_string());
                    //     }
                    // }
                    Err("Summary metric not supported".into())
                } else {
                    Err("Summary metric missing summary field".into())
                }
            }
            MetricType::Histogram => {
                if let Some(_histogram) = &proto_metric.histogram {
                    // let mut buckets = HashMap::new();
                    // for bucket in &histogram.bucket {
                    //     if let (Some(upper_bound), Some(count)) = (bucket.upper_bound, bucket.cumulative_count) {
                    //         let le_key = if upper_bound == f64::INFINITY {
                    //             "+Inf".to_string()
                    //         } else {
                    //             upper_bound.to_string()
                    //         };
                    //         buckets.insert(le_key, count.to_string());
                    //     }
                    // }
                    Err("Histogram metric not supported".into())
                } else {
                    Err("Histogram metric missing histogram field".into())
                }
            }
            _ => {
                // For untyped or unknown types
                let value = if let Some(untyped) = &proto_metric.untyped {
                    untyped.value.unwrap_or(0.0).to_string()
                } else {
                    "0".to_string()
                };

                Ok((labels, value))
            }
        }
    }

    pub fn parse_text_sensors(
        &self,
        content: &str,
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut sensors = HashMap::new();
        let lines: Vec<&str> = content.lines().collect();

        // iterate over all metric lines and create a sensor entry
        for line in lines
            .iter()
            .map(|l| l.trim())
            // filter out all empty lines and comments / metadata
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            // KISS parsing, just split on the first space and use the rest as value
            // Proper parsing would require at least some Regex
            if let Some((label_or_value, value_or_ts)) = line.rsplit_once(' ') {
                let key;
                let value;
                // watch out for timestamps!
                // They don't seem included in node_export, but when calling a Prometheus server!u
                if let Some((label, val)) = label_or_value.rsplit_once(' ')
                    && let Ok(number) = f64::from_str(val)
                {
                    key = label;
                    value = number.to_string();
                } else {
                    key = label_or_value;
                    value = if value_or_ts.len() > 4 && value_or_ts.contains('e') {
                        format!("{}", f64::from_str(value_or_ts)?)
                    } else {
                        value_or_ts.to_string()
                    };
                }

                // colon character is not allowed in key since it is used as a separator
                sensors.insert(key.replace(':', "_"), value);
            }
        }

        Ok(sensors)
    }

    pub fn parse_sensor_data(
        &self,
        data: &[u8],
        content_type: &str,
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        if Self::is_protobuf_format(content_type) {
            self.parse_protobuf_sensors(data)
        } else {
            let content = std::str::from_utf8(data)?;
            self.parse_text_sensors(content)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_metric_parsing() {
        let client = PromClient::new(ClientConfig {
            cert_path: None,
            key_path: None,
            accept_invalid_cert: false,
            connect_timeout: Default::default(),
            timeout: Default::default(),
        })
        .unwrap();

        let test_input = r#"
# HELP http_requests_total The total number of HTTP requests.
# TYPE http_requests_total counter
http_requests_total{method="post",code="200"} 1027
http_requests_total{method="post",code="400"} 3
"#;

        let sensors = client.parse_text_sensors(test_input).unwrap();
        let mut sensors = sensors.iter().sorted();
        assert_eq!(sensors.len(), 2);
        let (key, value) = sensors.next().unwrap();
        assert_eq!(key, r#"http_requests_total{method="post",code="200"}"#);
        assert_eq!(value, "1027");
        let (key, value) = sensors.next().unwrap();
        assert_eq!(key, r#"http_requests_total{method="post",code="400"}"#);
        assert_eq!(value, "3");
    }

    #[test]
    fn test_text_metric_parsing_with_timestamp() {
        let client = PromClient::new(ClientConfig {
            cert_path: None,
            key_path: None,
            accept_invalid_cert: false,
            connect_timeout: Default::default(),
            timeout: Default::default(),
        })
        .unwrap();

        let test_input = r#"
# HELP http_requests_total The total number of HTTP requests.
# TYPE http_requests_total counter
http_requests_total{method="post",code="200"} 1027 1395066363000
http_requests_total{method="post",code="400"} 3 1395066363000
"#;

        let sensors = client.parse_text_sensors(test_input).unwrap();
        let mut sensors = sensors.iter().sorted();
        assert_eq!(sensors.len(), 2);
        let (key, value) = sensors.next().unwrap();
        assert_eq!(key, r#"http_requests_total{method="post",code="200"}"#);
        assert_eq!(value, "1027");
        let (key, value) = sensors.next().unwrap();
        assert_eq!(key, r#"http_requests_total{method="post",code="400"}"#);
        assert_eq!(value, "3");
    }

    #[test]
    fn test_content_type_detection() {
        assert!(PromClient::is_protobuf_format(
            "application/vnd.google.protobuf"
        ));
        assert!(PromClient::is_protobuf_format(
            "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited"
        ));
        assert!(!PromClient::is_protobuf_format("text/plain"));
        assert!(!PromClient::is_protobuf_format("text/plain; version=0.0.4"));
    }
}
