pub use crulz::*;

use std::{
    collections::HashMap,
    io::{self, Write},
};

fn print_ast(step: &str, trs: &[crulz::ast::ASTNode]) {
    eprintln!(
        "crulz: {}:\n{:#?}\n----",
        ansi_term::Style::new().bold().paint(step),
        trs
    );
}

fn timing_of_intern(print_timings: bool, tbfx: std::time::Instant, fname: &'static str) {
    if print_timings {
        let elp = tbfx.elapsed().as_micros();
        if elp > 9 {
            eprintln!(
                "crulz: {}: {} {} μs",
                ansi_term::Style::new().bold().paint("timings"),
                fname,
                elp
            );
        }
    }
}

macro_rules! timing_of {
    ($print_timings:ident, $name:path, $fn:expr) => {{
        let now = std::time::Instant::now();
        let ret = $fn;
        timing_of_intern($print_timings, now, stringify!($name));
        ret
    }};
}

fn main() {
    use crate::mangle_ast::MangleAST;
    use clap::Arg;

    #[allow(unused_mut)]
    let mut matches = clap::App::new("crulz")
        .version(clap::crate_version!())
        .author("Erik Zscheile <zseri.devel@ytrizja.de>")
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
        );

    #[cfg(feature = "compile")]
    {
        matches = matches
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
            );
    }

    let matches = matches.get_matches();

    let escc = matches.value_of("escc").unwrap_or("\\");
    let escc = {
        let mut chx = escc.chars();
        let escc_aso = chx.next();
        if escc_aso == None || chx.next() != None {
            panic!("invalid escc argument");
        }
        let mut b = [0u8; 1];
        escc_aso.unwrap().encode_utf8(&mut b);
        b[0]
    };

    let escc_pass = matches.is_present("pass-escc");
    let vblvl = matches.occurrences_of("v");
    let print_timings = matches.is_present("timings");

    let input_file = matches.value_of_os("INPUT").unwrap().to_owned();
    let opts = parser::ParserOptions::new(escc, escc_pass);

    let mut trs = timing_of!(
        print_timings,
        parser::file2ast,
        parser::file2ast(std::path::Path::new(&input_file), opts)
            .expect("failed to parse input file")
    );

    if vblvl > 1 {
        print_ast("AST before evaluation", &trs);
    }

    cfg_if::cfg_if! {
        if #[cfg(feature = "compile")] {
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
        } else {
            timing_of!(
                print_timings,
                interp::eval,
                interp::eval(&mut trs, opts, &HashMap::new(), None)
            );
        }
    }

    if vblvl > 0 {
        print_ast("AST after evaluation", &trs);
    }

    if !matches.is_present("quiet") {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        stdout
            .write_all(&*trs.to_vec(escc))
            .expect("unable to write result");
        stdout.flush().expect("unable to flush result");
    }
}
