//! This module defines types and functions for submitting log entries
//! to the Stackdriver Logging API.
//!
//! Initially this will use the HTTP & JSON API instead of gRPC.
//!
//! Documentation for the relevant endpoint is available at:
//!    https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write

use chrono::{DateTime, Utc};
use failure::{Error, ResultExt};
use std::collections::HashMap;
use reqwest::Client;

const WRITE_ENTRY_URL: &str = "https://logging.googleapis.com/v2/entries:write";
const TOKEN_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

/// Represents an OAuth 2 access token as returned by Google's APIs.
#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonitoredResource {
    /// The monitored resource type. This field must match the type
    /// field of a MonitoredResourceDescriptor object.
    #[serde(rename = "type")]
    resource_type: String,

    /// Values for all of the labels listed in the associated
    /// monitored resource descriptor.
    labels: HashMap<String, String>,
}

/// This type represents a single Stackdriver log entry.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry<'a> {
    /// The resource name of the log to which this log entry belongs.
    log_name: String,

    /// The primary monitored resource associated with this log entry.
    resource: &'a MonitoredResource,

    /// The time the event described by the log entry occurred.
    timestamp: DateTime<Utc>,

    /// A set of user-defined (key, value) data that provides
    /// additional information about the log entry.
    labels: HashMap<String, String>,
}

/// This type represents the request sent to the Stackdriver API to
/// insert a batch of log records.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteEntriesRequest<'a> {
    /// The log entries to send to Stackdriver Logging.
    entries: Vec<LogEntry<'a>>,

    /// Whether valid entries should be written even if some other
    /// entries fail due to `INVALID_ARGUMENT` or `PERMISSION_DENIED`
    /// errors.
    partial_success: bool,

    /// Default labels that are added to the labels field of all log
    /// entries in entries. If a log entry already has a label with
    /// the same key as a label in this parameter, then the log
    /// entry's label is not changed.
    labels: HashMap<String, String>,

    /// The log entry payload, represented as a Unicode string.
    text_payload: String,
}

// Define the metadata header required by Google's service:
header! { (MetadataFlavor, "Metadata-Flavor") => [String] }

/// This function is used to fetch a new authentication token from
/// Google. Currently only tokens retrieved via instance metadata are
/// supported.
fn fetch_token() -> Result<Token, Error> {
    let token: Token = Client::new()
        .get(TOKEN_URL)
        .header(MetadataFlavor("Google".to_string()))
        .send().context("Requesting token failed")?
        .json().context("Deserializing token failed")?;

    Ok(token)
}
