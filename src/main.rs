#[macro_use] extern crate failure;
extern crate libc;

mod journald;

use std::process;

fn main() {
    let mut journal = match journald::open_journal() {
        Ok(journal) => journal,
        Err(e) => {
            println!("{}", e);
            process::exit(1);
        },
    };

    println!("foo");
    
    let entry = journal.read_next();

    println!("Entry: {:?}", entry)
}
