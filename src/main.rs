pub use crulz::*;

use gumdrop::Options;
use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
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

#[derive(Debug, Options)]
struct CrulzOptions {
    #[options(free)]
    inputs: Vec<String>,

    #[options(help = "prints help information")]
    help: bool,

    #[options(help = "sets the escape character")]
    escc: Option<u8>,

    #[options(help = "enable direct pass-through for double escape character")]
    pass_escc: bool,

    #[options(count, help = "sets the level of verbosity", short = "v")]
    verbose: u8,

    #[options(help = "output various timings / perf stats")]
    timings: bool,

    #[options(help = "suppress output of evaluated data")]
    quiet: bool,

    #[cfg(feature = "compile")]
    #[options(
        help = "each given element has the format '$1=$2' -> map '$1=$2' includes of $1 to $2"
    )]
    map_to_compilate: Vec<String>,

    #[cfg(feature = "compile")]
    #[options(
        help = "if set, writes the packed processed output including defines to the given output file"
    )]
    compile_output: Option<PathBuf>,

    #[options(help = "if set, writes the evaluated data to the given file")]
    output: Option<PathBuf>,
}

fn main() {
    use crulz::ast::MangleAST;

    let opts = CrulzOptions::parse_args_default_or_exit();

    let escc = opts.escc.unwrap_or(b'\\');
    let escc_pass = opts.pass_escc;
    let vblvl = opts.verbose;
    let print_timings = opts.timings;

    if opts.inputs.len() != 1 {
        eprintln!(
            "crulz: ERROR: expected exactly one input file, got {}",
            opts.inputs.len()
        );
        std::process::exit(1);
    }

    let input_file = opts.inputs[0].to_owned();
    let pars_opts = parser::ParserOptions::new(escc, escc_pass);

    let mut trs = timing_of!(
        print_timings,
        parser::file2ast,
        parser::file2ast(Path::new(&input_file), pars_opts).expect("failed to parse input file")
    );

    if vblvl > 1 {
        print_ast("AST before evaluation", &trs);
    }

    #[allow(unused_assignments, unused_mut)]
    let mut comp_map = HashMap::<PathBuf, PathBuf>::new();
    #[allow(unused_assignments, unused_mut)]
    let mut comp_out = None;

    cfg_if::cfg_if! {
        if #[cfg(feature = "compile")] {
            comp_map = opts.map_to_compilate
                .into_iter()
                .map(|y| {
                    let tmp: Vec<_> = y.split('=').take(2).collect();
                    (PathBuf::from(tmp[0]), PathBuf::from(tmp[1]))
                })
                .collect();
            comp_out = opts.compile_output.as_ref().map(|x| Path::new(x));
        }
    };

    let mut ectx = interp::EvalContext::new(
        pars_opts,
        comp_map
            .iter()
            .map(|(a, b)| (Path::new(a), Path::new(b)))
            .collect(),
    );

    timing_of!(
        print_timings,
        interp::eval,
        interp::eval(&mut trs, &mut ectx, comp_out,)
    );

    if vblvl > 0 {
        print_ast("AST after evaluation", &trs);
    }

    if opts.output.is_none() && opts.quiet {
        // we don't need to write processed output
        return;
    }

    let blob = trs.to_vec(escc);
    let blob = &*blob;

    if let Some(x) = opts.output.as_ref() {
        std::fs::write(x, blob).expect("unable to write result to given output file");
    }

    if !opts.quiet {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(blob).expect("unable to write result");
        stdout.flush().expect("unable to flush result");
    }
}
