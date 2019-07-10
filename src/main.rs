#![cfg_attr(test, feature(test))]

extern crate ansi_term;
extern crate clap;
extern crate failure;
extern crate rayon;
extern crate readfilez;
extern crate sharpen;

#[macro_use]
mod ast;
mod interp;
mod lexer;
mod mangle_ast;
mod parser;

use ansi_term::{Colour, Style};
use std::{io, io::Write};

pub fn errmsg(s: &str) {
    eprintln!("crulz: {}: {}", Colour::Red.bold().paint("ERROR"), s);
    std::process::exit(1);
}

pub fn notemsg(cat: &str, s: &str) {
    eprintln!("crulz: {}: {}", Style::new().bold().paint(cat), s);
}

macro_rules! timing_of {
    ($print_timings:ident, $name:path, $fn:expr) => {{
        let now = std::time::Instant::now();
        let ret = $fn;
        if $print_timings {
            let elp = now.elapsed().as_micros();
            if elp > 9 {
                notemsg("timings", &format!("{} {} μs", stringify!($name), elp));
            }
        }
        ret
    }};
}

fn main() {
    use crate::mangle_ast::MangleAST;
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
            Arg::with_name("pass-escc")
                .short("p")
                .long("pass-escc")
                .help("if set, double escape character gets passed through"),
        )
        .arg(
            Arg::with_name("timings")
                .short("t")
                .long("timings")
                .help("if set, output various timings"),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("if set, suppress output of evaluated data"),
        )
        .get_matches();

    let escc = matches.value_of("escc").unwrap_or("\\").as_bytes();
    if escc.len() != 1 {
        errmsg("invalid escc argument");
    }
    let escc = escc[0] as u8;

    let escc_pass = matches.is_present("pass-escc");
    let vblvl = matches.occurrences_of("v");
    let print_timings = matches.is_present("timings");

    let input_file = matches.value_of("INPUT").unwrap().to_owned();

    let mut trs = timing_of!(
        print_timings,
        parser::file2ast,
        parser::file2ast(input_file, escc, escc_pass).expect("crulz: failed to parse input file")
    );

    if vblvl > 1 {
        notemsg("AST before evaluation", "");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    timing_of!(
        print_timings,
        interp::eval,
        interp::eval(&mut trs, escc, escc_pass)
    );

    if vblvl > 0 {
        notemsg("AST after evaluation", "");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    if !matches.is_present("quiet") {
        let rsb = trs.to_u8v(escc);
        io::stdout()
            .write_all(&rsb)
            .expect("unable to write reser-result");
    }
}
