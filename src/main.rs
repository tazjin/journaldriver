// #[macro_use] extern crate failure;
#[macro_use] extern crate log;

extern crate env_logger;
extern crate systemd;

use systemd::journal::*;
use std::process;
use std::thread;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use std::collections::vec_deque::{VecDeque, Drain};

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

/// This function spawns a double-looped, blocking receiver. It will
/// buffer messages for a second before flushing them to Stackdriver.
fn receiver_loop(rx: Receiver<Record>) {
    let mut buf = VecDeque::new();
    let iteration = Duration::from_millis(500);

    loop {
        trace!("Beginning outer iteration");
        let now = Instant::now();

        loop {
            if now.elapsed() > iteration {
                break;
            }

            if let Ok(record) = rx.recv_timeout(iteration) {
                buf.push_back(record);
            }
        }

        if !buf.is_empty() {
            flush(buf.drain(..));
        }

        trace!("Done outer iteration");
    }
}

/// Flushes all drained records to Stackdriver.
fn flush(drain: Drain<Record>) {
    let record_count = drain.count();
    debug!("Flushed {} records", record_count);
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

    let (tx, rx) = channel();

    let _receiver = thread::spawn(move || receiver_loop(rx));

    journal.watch_all_elements(move |record| {
        let record: Record = record.into();

        if record.message.is_some() {
            tx.send(record).ok();
        }

        Ok(())
    }).expect("Failed to read new entries from journal");
}
