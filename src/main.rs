//! This file implements journaldriver, a small application that
//! forwards logs from journald (systemd's log facility) to
//! Stackdriver Logging.
//!
//! Log entries are read continously from journald and are forwarded
//! to Stackdriver in batches.
//!
//! Stackdriver Logging has a concept of monitored resources. In the
//! simplest (and currently only supported) case this monitored
//! resource will be the GCE instance on which journaldriver is
//! running.
//!
//! Information about the instance, the project and required security
//! credentials are retrieved from Google's metadata instance on GCP.
//!
//! Things left to do:
//! * TODO 2018-06-15: Support non-GCP instances (see comment on
//!   monitored resource descriptor)
//! * TODO 2018-06-15: Extract timestamps from journald instead of
//!   relying on ingestion timestamps.
//! * TODO 2018-06-15: Persist last known cursor position after
//!   flushing to allow journaldriver to resume from the same position
//!   after a restart.

#[macro_use] extern crate hyper;
#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
#[macro_use] extern crate serde_json;
#[macro_use] extern crate lazy_static;

extern crate failure;
extern crate env_logger;
extern crate systemd;
extern crate serde;
extern crate reqwest;

use reqwest::{header, Client};
use serde_json::Value;
use std::io::Read;
use std::mem;
use std::process;
use std::time::{Duration, Instant};
use systemd::journal::*;

const ENTRIES_WRITE_URL: &str = "https://logging.googleapis.com/v2/entries:write";
const METADATA_TOKEN_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const METADATA_ID_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/id";
const METADATA_ZONE_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/zone";
const METADATA_PROJECT_URL: &str = "http://metadata.google.internal/computeMetadata/v1/project/project-id";

// Google's metadata service requires this header to be present on all
// calls:
//g
// https://cloud.google.com/compute/docs/storing-retrieving-metadata#querying
header! { (MetadataFlavor, "Metadata-Flavor") => [String] }

/// Convenience type alias for results using failure's `Error` type.
type Result<T> = std::result::Result<T, failure::Error>;

lazy_static! {
    /// HTTP client instance preconfigured with the metadata header
    /// required by Google.
    static ref METADATA_CLIENT: Client = {
        let mut headers = header::Headers::new();
        headers.set(MetadataFlavor("Google".into()));

        Client::builder().default_headers(headers)
            .build().expect("Could not create metadata client")
    };

    /// ID of the GCP project in which this instance is running.
    static ref PROJECT_ID: String = get_metadata(METADATA_PROJECT_URL)
        .expect("Could not determine project ID");

    /// ID of the current GCP instance.
    static ref INSTANCE_ID: String = get_metadata(METADATA_ID_URL)
        .expect("Could not determine instance ID");

    /// GCP zone in which this instance is running.
    static ref ZONE: String = get_metadata(METADATA_ZONE_URL)
        .expect("Could not determine instance zone");

    /// Descriptor of the currently monitored instance.
    ///
    /// For GCE instances, this will be the GCE instance ID. For
    /// non-GCE machines a sensible solution may be using the machine
    /// hostname as a Cloud Logging log name, but this is not yet
    /// implemented.
    static ref MONITORED_RESOURCE: Value = json!({
        "type": "gce_instance",
        "labels": {
            "project_id": PROJECT_ID.as_str(),
            "instance_id": INSTANCE_ID.as_str(),
            "zone": ZONE.as_str(),
        }
    });
}

/// Convenience helper for retrieving values from the metadata server.
fn get_metadata(url: &str) -> Result<String> {
    let mut output = String::new();
    METADATA_CLIENT.get(url).send()?
        .error_for_status()?
        .read_to_string(&mut output)?;

    Ok(output.trim().into())
}

/// This structure represents a log entry in the format expected by
/// the Stackdriver API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    labels: Value,
    text_payload: String, // TODO: attempt to parse jsonPayloads
}

impl From<JournalRecord> for LogEntry {
    // Converts from the fields contained in a journald record to the
    // representation required by Stackdriver Logging.
    //
    // The fields are documented in systemd.journal-fields(7).
    fn from(mut record: JournalRecord) -> LogEntry {
        // The message field is technically just a convention, but
        // journald seems to default to it when ingesting unit
        // output.
        let message = record.remove("MESSAGE");

        // Presumably this is always set, but who can be sure
        // about anything in this world.
        let hostname = record.remove("_HOSTNAME");

        // The unit is seemingly missing on kernel entries, but
        // present on all others.
        let unit = record.remove("_SYSTEMD_UNIT");

        // TODO: This timestamp (in microseconds) should be parsed
        // into a DateTime<Utc> and used instead of the ingestion
        // time.
        // let timestamp = record
        //     .remove("_SOURCE_REALTIME_TIMESTAMP")
        //     .map();

        LogEntry {
            text_payload: message.unwrap_or_else(|| "empty log entry".into()),
            labels: json!({
                "host": hostname,
                "unit": unit.unwrap_or_else(|| "syslog".into()),
            }),
        }
    }
}

/// This function starts a double-looped, blocking receiver. It will
/// buffer messages for half a second before flushing them to
/// Stackdriver.
fn receiver_loop(mut journal: Journal) -> Result<()> {
    let mut token = get_metadata_token()?;
    let client = reqwest::Client::new();

    let mut buf: Vec<LogEntry> = Vec::new();
    let iteration = Duration::from_millis(500);

    loop {
        trace!("Beginning outer iteration");
        let now = Instant::now();

        loop {
            if now.elapsed() > iteration {
                break;
            }

            if let Ok(Some(entry)) = journal.await_next_record(Some(iteration)) {
                trace!("Received a new entry");
                buf.push(entry.into());
            }
        }

        if !buf.is_empty() {
            let to_flush = mem::replace(&mut buf, Vec::new());
            flush(&client, &mut token, to_flush)?;
        }

        trace!("Done outer iteration");
    }
}

/// Flushes all drained records to Stackdriver. Any Stackdriver
/// message can at most contain 1000 log entries which means they are
/// chunked up here.
fn flush(client: &Client, token: &mut Token, entries: Vec<LogEntry>) -> Result<()> {
    if token.is_expired() {
        debug!("Refreshing Google metadata access token");
        let new_token = get_metadata_token()?;
        mem::replace(token, new_token);
    }

    for chunk in entries.chunks(1000) {
        let request = prepare_request(chunk);
        if let Err(write_error) = write_entries(client, token, request) {
            error!("Failed to write {} entries: {}", chunk.len(), write_error)
        } else {
            debug!("Wrote {} entries to Stackdriver", chunk.len())
        }
    }

    Ok(())
}

/// Represents the response returned by the metadata server's token
/// endpoint. The token is normally valid for an hour.
#[derive(Deserialize)]
struct TokenResponse {
    expires_in: u64,
    access_token: String,
}

/// Struct used to store a token together with a sensible
/// representation of when it expires.
struct Token {
    token: String,
    fetched_at: Instant,
    expires: Duration,
}

impl Token {
    /// Does this token need to be renewed?
    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.expires
    }
}

fn get_metadata_token() -> Result<Token> {
    let token: TokenResponse  = METADATA_CLIENT
        .get(METADATA_TOKEN_URL)
        .send()?.json()?;

    debug!("Fetched new token from metadata service");

    Ok(Token {
        fetched_at: Instant::now(),
        expires: Duration::from_secs(token.expires_in / 2),
        token: token.access_token,
    })
}

/// Convert a slice of log entries into the format expected by
/// Stackdriver. This format is documented here:
///
/// https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write
fn prepare_request(entries: &[LogEntry]) -> Value {
    json!({
        "logName": format!("projects/{}/logs/journaldriver", PROJECT_ID.as_str()),
        "resource": &*MONITORED_RESOURCE,
        "entries": entries,
        "partialSuccess": true
    })
}

/// Perform the log entry insertion in Stackdriver Logging.
fn write_entries(client: &Client, token: &Token, request: Value) -> Result<()> {
    client.post(ENTRIES_WRITE_URL)
        .header(header::Authorization(format!("Bearer {}", token.token)))
        .json(&request)
        .send()?
        .error_for_status()?;

    Ok(())
}

fn main () {
    env_logger::init();

    let mut journal = Journal::open(JournalFiles::All, false, true)
        .expect("Failed to open systemd journal");

    match journal.seek(JournalSeek::Tail) {
        Ok(cursor) => info!("Opened journal at cursor '{}'", cursor),
        Err(err) => {
            error!("Failed to set initial journal position: {}", err);
            process::exit(1)
        }
    }

    receiver_loop(journal).expect("log receiver encountered an unexpected error");
}
