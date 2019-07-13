#![cfg_attr(test, feature(test))]

#[macro_use]
#[allow(dead_code, clippy::unreadable_literal)]
mod crulst {
    include!(concat!(env!("OUT_DIR"), "/crulst_atom.rs"));
}

mod ast;
mod interp;
mod mangle_ast;
mod parser;

use ansi_term::{Colour, Style};
use hashbrown::HashMap;
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
        .arg(
            Arg::with_name("map-to-compilate")
                .short("m")
                .long("map-to-compilate")
                .multiple(true)
                .takes_value(true)
                .help("if set, map includes of $1 to $2"),
        )
        .arg(
            Arg::with_name("compile-output")
                .short("c")
                .long("compile-output")
                .takes_value(true)
                .help("if set, writes the processed output including defines to the given output file"),
        )
        .get_matches();

    let escc = matches.value_of("escc").unwrap_or("\\");
    let escc = {
        let mut chx = escc.chars();
        let escc_aso = chx.next();
        if escc_aso == None || chx.next() != None {
            errmsg("invalid escc argument");
        }
        escc_aso.unwrap()
    };

    let escc_pass = matches.is_present("pass-escc");
    let vblvl = matches.occurrences_of("v");
    let print_timings = matches.is_present("timings");

    let input_file = matches.value_of("INPUT").unwrap().to_owned();
    let opts = parser::ParserOptions::new(escc, escc_pass);

    let mut trs = timing_of!(
        print_timings,
        parser::file2ast,
        parser::file2ast(&input_file, opts).expect("crulz: failed to parse input file")
    );

    if vblvl > 1 {
        notemsg("AST before evaluation", "");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    let comp_map = matches
        .values_of("map-to-compilate")
        .map(|x| {
            x.map(|y| {
                let tmp: Vec<_> = y.split('=').take(2).collect();
                (tmp[0], tmp[1])
            })
            .collect()
        })
        .unwrap_or_else(HashMap::new);

    timing_of!(
        print_timings,
        interp::eval,
        interp::eval(
            &mut trs,
            opts,
            &comp_map,
            matches.value_of("compile-output")
        )
    );

    if vblvl > 0 {
        notemsg("AST after evaluation", "");
        eprintln!("{:#?}", &trs);
        eprintln!("----");
    }

    if !matches.is_present("quiet") {
        print!("{}", trs.to_str(escc));
        io::stdout().flush().expect("unable to flush result");
    }
}
