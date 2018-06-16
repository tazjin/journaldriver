// Copyright (C) 2018  Aprila Bank ASA (contact: vincent@aprila.no)
//
// journaldriver is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

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

use failure::ResultExt;
use reqwest::{header, Client};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, ErrorKind, Write};
use std::mem;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, Instant};
use systemd::journal::*;

#[cfg(test)]
mod tests;

const ENTRIES_WRITE_URL: &str = "https://logging.googleapis.com/v2/entries:write";
const METADATA_TOKEN_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const METADATA_ID_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/id";
const METADATA_ZONE_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/zone";
const METADATA_PROJECT_URL: &str = "http://metadata.google.internal/computeMetadata/v1/project/project-id";

// Google's metadata service requires this header to be present on all
// calls:
//
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

    /// Path to the file in which journaldriver should persist its
    /// cursor state.
    static ref POSITION_FILE: PathBuf = env::var("CURSOR_POSITION_FILE")
        .unwrap_or("/var/journaldriver/cursor.pos".into())
        .into();
}

/// Convenience helper for retrieving values from the metadata server.
fn get_metadata(url: &str) -> Result<String> {
    let mut output = String::new();
    METADATA_CLIENT.get(url).send()?
        .error_for_status()?
        .read_to_string(&mut output)?;

    Ok(output.trim().into())
}


/// This structure represents the different types of payloads
/// supported by journaldriver.
///
/// Currently log entries can either contain plain text messages or
/// structured payloads in JSON-format.
#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
enum Payload {
    TextPayload {
        #[serde(rename = "textPayload")]
        text_payload: String,
    },
    JsonPayload {
        #[serde(rename = "jsonPaylaod")]
        json_payload: Value,
    },
}

/// Attempt to parse a log message as JSON and return it as a
/// structured payload. If parsing fails, return the entry in plain
/// text format.
fn message_to_payload(message: Option<String>) -> Payload {
    match message {
        None => Payload::TextPayload { text_payload: "empty log entry".into() },
        Some(text_payload) => {
            // Attempt to deserialize the text payload as a generic
            // JSON value.
            if let Ok(json_payload) = serde_json::from_str::<Value>(&text_payload) {
                // If JSON-parsing succeeded on the payload, check
                // whether we parsed an object (Stackdriver does not
                // expect other types of JSON payload) and return it
                // in that case.
                if json_payload.is_object() {
                    return Payload::JsonPayload { json_payload }
                }
            }

            Payload::TextPayload { text_payload }
        }
    }
}

/// This structure represents a log entry in the format expected by
/// the Stackdriver API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    labels: Value,
    #[serde(flatten)]
    payload: Payload,
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
        let payload = message_to_payload(record.remove("MESSAGE"));

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
            payload,
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
            flush(&client, &mut token, to_flush, journal.cursor()?)?;
        }

        trace!("Done outer iteration");
    }
}

/// Writes the current cursor into `/var/journaldriver/cursor.pos`.
fn persist_cursor(cursor: String) -> Result<()> {
    let mut file = File::create(&*POSITION_FILE)?;
    write!(file, "{}", cursor).map_err(Into::into)
}

/// Flushes all drained records to Stackdriver. Any Stackdriver
/// message can at most contain 1000 log entries which means they are
/// chunked up here.
///
/// If flushing is successful the last cursor position will be
/// persisted to disk.
fn flush(client: &Client,
         token: &mut Token,
         entries: Vec<LogEntry>,
         cursor: String) -> Result<()> {
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

    persist_cursor(cursor)
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

/// Attempt to read the initial cursor position from the configured
/// file. If there is no initial cursor position set, read from the
/// tail of the log.
///
/// The only "acceptable" error when reading the cursor position is
/// the cursor position file not existing, other errors are fatal
/// because they indicate a misconfiguration of journaldriver.
fn initial_cursor() -> Result<JournalSeek> {
    let read_result: io::Result<String> = (|| {
        let mut contents = String::new();
        let mut file = File::open(&*POSITION_FILE)?;
        file.read_to_string(&mut contents)?;
        Ok(contents.trim().into())
    })();

    match read_result {
        Ok(cursor) => Ok(JournalSeek::Cursor { cursor }),
        Err(ref err) if err.kind() == ErrorKind::NotFound => {
            info!("No previous cursor position, reading from journal tail");
            Ok(JournalSeek::Tail)
        },
        Err(err) => {
            (Err(err).context("Could not read cursor position"))?
        }
    }
}

fn main () {
    env_logger::init();

    // If the cursor file does not yet exist, the directory structure
    // leading up to it should be created:
    let cursor_position_dir = POSITION_FILE.parent()
        .expect("Invalid cursor position file path");

    fs::create_dir_all(cursor_position_dir)
        .expect("Could not create directory to store cursor position in");

    let mut journal = Journal::open(JournalFiles::All, false, true)
        .expect("Failed to open systemd journal");

    let seek_position = initial_cursor()
        .expect("Failed to determine initial cursor position");

    match journal.seek(seek_position) {
        Ok(cursor) => info!("Opened journal at cursor '{}'", cursor),
        Err(err) => {
            error!("Failed to set initial journal position: {}", err);
            process::exit(1)
        }
    }

    receiver_loop(journal).expect("log receiver encountered an unexpected error");
}
