#![cfg_attr(test, feature(test))]

extern crate clap;
extern crate rayon;

#[macro_use]
mod hlparser;
mod interp;
mod llparser;
mod sharpen;

use std::{io, io::Write};

pub fn errmsg(s: &str) {
    eprintln!("crulz: ERROR: {}", s);
    std::process::exit(1);
}

fn main() {
    use crate::hlparser::MangleAST;
    use clap::Arg;

    let matches = clap::App::new("crulz")
        .version("0.0.1")
        .author("Erik Zscheile <erik.zscheile@gmail.com>")
        .about("a macro language parser + interpreter")
        .arg(
            Arg::with_name("INPUT")
                .help("sets the input file to use")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("escc")
                .short("e")
                .long("escc")
                .takes_value(true)
                .help("sets the escape character"),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("sets the level of verbosity"),
        )
        .get_matches();

    let escc = matches.value_of("escc").unwrap_or("\\").as_bytes();
    if escc.len() != 1 {
        errmsg("invalid escc argument");
    }
    let escc = escc[0] as u8;

    let vblvl = matches.occurrences_of("v");

    let input_file = matches.value_of("INPUT").unwrap().to_owned();

    let mut trs = crossparse!(llparser::file2secs, input_file, escc);

    if vblvl > 1 {
        eprintln!("crulz: AST before evaluation:");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    interp::eval(&mut trs);

    if vblvl > 0 {
        eprintln!("crulz: AST after evaluation:");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    let rsb = trs.to_u8v(escc);
    io::stdout()
        .write_all(&rsb)
        .expect("unable to write reser-result");
}
