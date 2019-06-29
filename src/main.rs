#[macro_use]
mod hlparser;
mod llparser;

use hlparser::ToU8Vc;
use llparser::file2secs;
use std::{io, io::Write};

pub fn errmsg(s: &str) {
    let res = writeln!(io::stderr(), "crulz: ERROR: {}", s);
    std::process::exit(if let Err(_) = res { 2 } else { 1 });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut escc = '\\' as u8;

    if args.len() > 2 && &args[1] == "-e" {
        let a2 = args[2].as_bytes();
        if a2.len() != 1 {
            errmsg("invalid escc argument");
        }
        escc = a2[0];
    }

    if args.len() < 2 {
        println!("USAGE: crulz [-e ESCC] INPUT");
        std::process::exit(1);
    }

    let trs = crossparse!(file2secs, &args[1], escc);

    println!("{:#?}", &trs);

    let rsb = trs.to_u8v(escc);
    io::stdout()
        .write_all(&rsb)
        .expect("unable to write reser-result");
}
