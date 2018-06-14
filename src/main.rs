#[macro_use] extern crate failure;
#[macro_use] extern crate hyper;
#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;

extern crate chrono;
extern crate env_logger;
extern crate systemd;
extern crate serde;
extern crate serde_json;
extern crate reqwest;

mod stackdriver;

use std::env;
use std::mem;
use std::ops::Add;
use std::process;
use std::time::{Duration, Instant};
use systemd::journal::*;

const METADATA_TOKEN_URL: &str = "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

header! { (MetadataFlavor, "Metadata-Flavor") => [String] }

type Result<T> = std::result::Result<T, failure::Error>;

#[derive(Debug)]
struct Record {
    message: Option<String>,
    hostname: Option<String>,
    unit: Option<String>,
    timestamp: Option<String>,
}

impl From<JournalRecord> for Record {
    fn from(mut record: JournalRecord) -> Record {
        Record {
            // The message field is technically just a convention, but
            // journald seems to default to it when ingesting unit
            // output.
            message: record.remove("MESSAGE"),

            // Presumably this is always set, but who can be sure
            // about anything in this world.
            hostname: record.remove("_HOSTNAME"),

            // The unit is seemingly missing on kernel entries, but
            // present on all others.
            unit: record.remove("_SYSTEMD_UNIT"),

            // This timestamp is present on most log entries
            // (seemingly all that are ingested from the output
            // systemd units).
            timestamp: record.remove("_SOURCE_REALTIME_TIMESTAMP"),
        }
    }
}

/// This function starts a double-looped, blocking receiver. It will
/// buffer messages for half a second before flushing them to
/// Stackdriver.
fn receiver_loop(mut journal: Journal) {
    let mut buf: Vec<Record> = Vec::new();
    let iteration = Duration::from_millis(500);

    loop {
        trace!("Beginning outer iteration");
        let now = Instant::now();

        loop {
            if now.elapsed() > iteration {
                break;
            }

            if let Ok(Some(record)) = journal.await_next_record(Some(iteration)) {
                trace!("Received a new record");
                buf.push(record.into());
            }
        }

        if !buf.is_empty() {
            let to_flush = mem::replace(&mut buf, Vec::new());
            flush(to_flush);
        }

        trace!("Done outer iteration");
    }
}

/// Flushes all drained records to Stackdriver. Any Stackdriver
/// message can at most contain 1000 log entries which means they are
/// chunked up here.
fn flush(records: Vec<Record>) {
    for chunk in records.chunks(1000) {
        debug!("Flushed {} records", chunk.len())
    }
}

/// Retrieves an access token from the GCP metadata service.
#[derive(Deserialize)]
struct TokenResponse {
    #[serde(rename = "type")]
    expires_in: u64,
    access_token: String,
}

/// Struct used to store a token together with a sensible
/// representation of when it expires.
struct Token {
    token: String,
    renew_at: Instant,
}

fn get_metadata_token(client: &reqwest::Client) -> Result<Token> {
    let now = Instant::now();

    let token: TokenResponse  = client.get(METADATA_TOKEN_URL)
        .header(MetadataFlavor("Google".into()))
        .send()?.json()?;

    debug!("Fetched new token from metadata service");

    let renew_at = now.add(Duration::from_secs(token.expires_in / 2));

    Ok(Token {
        renew_at,
        token: token.access_token,
    })
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

    receiver_loop(journal)
}
