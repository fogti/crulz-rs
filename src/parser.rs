use crate::sharpen::*;

use crate::lexer::LowerLexerToken;
type LLT = LowerLexerToken;

#[derive(Copy, Clone, Debug, PartialEq)]
enum LLCPR {
    Normal,
    NormalSpace,
    Grouped,
    Escaped,
}

impl std::default::Default for LLCPR {
    fn default() -> Self {
        LLCPR::Normal
    }
}

#[derive(Debug, PartialEq)]
pub enum SectionType {
    Normal,
    NormalSpace,
    Grouped,
    CmdEval,
}

use crate::ast::VAN;

type ParserResult = Result<VAN, failure::Error>;

fn run_parser(input: &[LLT], escc: u8, pass_escc: bool) -> ParserResult {
    // we should be able to parse non-utf8 input,
    // as long as the parts starting with ESCC '(' ( and ending with ')')
    // are valid utf8

    let mut is_escaped = false;
    let mut flipp = false;
    let mut clret = LLCPR::Normal;
    let mut nesting: usize = 0;

    use std::time::Instant;

    let now = Instant::now();
    let ret = input
        .into_iter()
        .copied()
        .classify(|i| {
            if is_escaped {
                is_escaped = false;
                match i {
                    LowerLexerToken::Paren(true) => {
                        // we can't do 'nesting += 1;' here,
                        // because we want '\(...)' in one blob
                    }
                    LowerLexerToken::Paren(false) => {
                        // '('
                        panic!("crulz: ERROR: got dangerous '\\)'");
                    }
                    _ => {
                        nesting = 0;
                        clret = LLCPR::Normal;
                        flipp ^= true;
                        return (!flipp, LLCPR::Escaped);
                    }
                }
            } else if nesting == 0 {
                clret = LLCPR::Normal;
                match i {
                    LowerLexerToken::Escape(_) => {
                        clret = LLCPR::Escaped;
                        is_escaped = true;
                    }
                    LowerLexerToken::Paren(true) => {
                        clret = LLCPR::Grouped;
                    }
                    LowerLexerToken::Paren(false) => {
                        // '('
                        panic!("crulz: ERROR: unexpected unbalanced ')'");
                    }
                    _ => {}
                }
                if clret != LLCPR::Normal {
                    nesting = 1;
                    flipp ^= true;
                }
            } else {
                // grouped
                match i {
                    LowerLexerToken::Paren(true) => {
                        nesting += 1;
                    }
                    LowerLexerToken::Paren(false) => {
                        nesting -= 1;
                    }
                    _ => {}
                }
            }
            (
                flipp,
                if clret == LLCPR::Normal && i.is_space() {
                    LLCPR::NormalSpace
                } else {
                    clret
                },
            )
        })
        .map(|((_, d), section)| {
            assert!(!section.is_empty());
            let slen = section.len();
            let (stype, section) = match d {
                LLCPR::Escaped if !pass_escc && slen == 2 => {
                    (SectionType::Normal, std::slice::from_ref(&section[1]))
                }
                LLCPR::Escaped
                    if slen > 2
                        && section[1] == LowerLexerToken::Paren(true)
                        && *section.last().unwrap() == LowerLexerToken::Paren(false) =>
                {
                    if slen == 3 {
                        panic!("crulz: ERROR: got empty eval stmt");
                    }
                    (SectionType::CmdEval, &section[2..slen - 1])
                }
                LLCPR::Grouped => (SectionType::Grouped, &section[1..slen - 1]),
                LLCPR::NormalSpace => (SectionType::NormalSpace, &section[..]),
                _ => (SectionType::Normal, &section[..]),
            };
            use crate::ast::ASTNode::*;
            match stype {
                SectionType::CmdEval => {
                    let first_space = section.iter().position(|&x| x.is_space());
                    let rest = first_space.map(|x| &section[x + 1..]).unwrap_or(&[]);

                    CmdEval(
                        std::str::from_utf8(
                            &section[0..first_space.unwrap_or_else(|| section.len())]
                                .iter()
                                .map(std::convert::Into::<u8>::into)
                                .collect::<Vec<_>>(),
                        )
                        .expect("cmdeval utf8-conv failed")
                        .to_owned(),
                        run_parser(rest, escc, pass_escc).expect("sub-parser failed"),
                    )
                }
                SectionType::Grouped => Grouped(
                    true,
                    run_parser(&section, escc, pass_escc).expect("sub-parser failed"),
                ),
                SectionType::Normal | SectionType::NormalSpace => Constant(
                    stype == SectionType::Normal,
                    section
                        .into_iter()
                        .map(std::convert::Into::<u8>::into)
                        .collect(),
                ),
            }
        })
        .collect::<Vec<_>>();

    let parse_timing = now.elapsed().as_micros();
    if parse_timing != 0 {
        println!("run_parser {} μs", parse_timing);
    }

    if nesting != 0 {
        panic!("crulz ERROR: unexpected EOF");
    }

    Ok(ret)
}

pub fn file2ast(filename: String, escc: u8, pass_escc: bool) -> ParserResult {
    run_parser(
        &crate::lexer::lex(
            readfilez::read_from_file(std::fs::File::open(filename))?.get_slice(),
            escc,
        ),
        escc,
        pass_escc,
    )
}
