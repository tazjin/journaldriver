//! This module contains FFI-bindings to the journald APi. See
//! sd-journal(3) for detailed information about the API.
//!
//! Only calls required by journaldriver are implemented.

/// This type represents an opaque pointer to an `sd_journal` struct.
/// It should be changed to an `extern type` once RF1861 is
/// stabilized.
enum SdJournal {}

use failure::Error;
use std::mem;

extern {
    fn sd_journal_open(sd_journal: *mut SdJournal, flags: usize) -> usize;
    fn sd_journal_close(sd_journal: *mut SdJournal);
    fn sd_journal_next(sd_journal: *mut SdJournal) -> usize;
}

// Safe interface:

/// This struct contains the opaque data used by libsystemd to
/// reference the journal.
pub struct Journal {
    sd_journal: *mut SdJournal,
}

impl Drop for Journal {
    fn drop(&mut self) {
        unsafe {
            sd_journal_close(self.sd_journal);
        }
    }
}

/// Open the journal for reading. No flags are supplied to libsystemd,
/// which means that all journal entries will become available.
pub fn open_journal() -> Result<Journal, Error> {
    let (mut sd_journal, result) = unsafe {
        let mut journal: SdJournal = mem::uninitialized();
        let result = sd_journal_open(&mut journal, 0);
        (journal, result)
    };

    ensure!(result == 0, "Could not open journal (errno: {})", result);
    Ok(Journal { sd_journal: &mut sd_journal })
}

#[derive(Debug)]
pub enum NextEntry {
    /// If no new entries are available in the journal this variant is
    /// returned.
    NoEntry,

    Entry,
}

impl Journal {
    pub fn read_next(&self) -> Result<NextEntry, Error> {
        let result = unsafe {
            sd_journal_next(self.sd_journal)
        };

        match result {
            0 => Ok(NextEntry::NoEntry),
            1 => Ok(NextEntry::Entry),
            n if n > 1 => bail!("Journal unexpectedly advanced by {} entries!", n),
            _ => bail!("An error occured while advancing the journal (errno: {})", result),
        }
    }
}
