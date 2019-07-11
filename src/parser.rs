use crate::ast::VAN;
use crate::lexer::{LowerLexerToken, LexerToken};
use sharpen::*;

type LT = LexerToken;

#[derive(Copy, Clone, Debug, PartialEq)]
enum LLCPR {
    Normal,
    NormalSpace,
    Grouped,
    Escaped,
    CmdEval,
}

impl std::default::Default for LLCPR {
    fn default() -> Self {
        LLCPR::Normal
    }
}

type ParserResult = Result<VAN, failure::Error>;

fn section2u8v(input: &[LT]) -> Vec<u8> {
    input.iter().map(|i| std::convert::Into::<u8>::into(i.llt)).collect()
}

fn run_parser(input: Vec<LT>, escc: u8, pass_escc: bool) -> ParserResult {
    // we should be able to parse non-utf8 input,
    // as long as the parts starting with ESCC '(' ( and ending with ')')
    // are valid utf8

    let mut is_escaped = false;
    let mut flipp = false;
    let mut clret = LLCPR::Normal;
    let mut nesting: usize = 0;

    let ret = input
        .into_iter()
        .classify(|i| {
            let j = i.llt;
            if is_escaped {
                is_escaped = false;
                match j {
                    LowerLexerToken::Paren(true) => {
                        // we can't do 'nesting += 1;' here,
                        // because we want '\(...)' in one blob
                    }
                    LowerLexerToken::Paren(false) => {
                        // '('
                        crate::errmsg(&format!("got dangerous '\\)' at {}", i.pos));
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
                match j {
                    LowerLexerToken::Escape(_) => {
                        clret = LLCPR::Escaped;
                        is_escaped = true;
                    }
                    LowerLexerToken::Paren(true) => {
                        clret = LLCPR::Grouped;
                    }
                    LowerLexerToken::Paren(false) => {
                        // '('
                        crate::errmsg(&format!("unexpected unbalanced ')' at {}", i.pos));
                    }
                    _ => {}
                }
                if clret != LLCPR::Normal {
                    nesting = 1;
                    flipp ^= true;
                }
            } else {
                // grouped
                match j {
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
                    (LLCPR::Normal, std::slice::from_ref(&section[1]))
                }
                LLCPR::Escaped
                    if slen > 2
                        && section[1].llt == LowerLexerToken::Paren(true)
                        && section.last().unwrap().llt == LowerLexerToken::Paren(false) =>
                {
                    if slen == 3 {
                        panic!("crulz: ERROR: got empty eval stmt");
                    }
                    (LLCPR::CmdEval, &section[2..slen - 1])
                }
                LLCPR::Grouped => (d, &section[1..slen - 1]),
                _ => (d, &section[..]),
            };
            use crate::ast::ASTNode::*;
            Ok(match stype {
                LLCPR::CmdEval => {
                    let first_space = section.iter().position(|x| x.is_space());
                    CmdEval(
                        std::str::from_utf8(&section2u8v(
                            &section[0..first_space.unwrap_or_else(|| section.len())],
                        ))?
                        .to_owned(),
                        run_parser(
                            first_space.map(|x| section[x + 1..].to_vec()).unwrap_or(Vec::new()),
                            escc,
                            pass_escc,
                        )?,
                    )
                }
                LLCPR::Grouped => Grouped(true, run_parser(section.to_vec(), escc, pass_escc)?),
                LLCPR::Normal | LLCPR::NormalSpace | LLCPR::Escaped => {
                    Constant(stype != LLCPR::NormalSpace, section2u8v(&section[..]))
                }
            })
        })
        .collect::<ParserResult>();

    if nesting != 0 {
        crate::errmsg("unexpected EOF");
    }

    ret
}

pub fn file2ast(filename: String, escc: u8, pass_escc: bool) -> ParserResult {
    let ret = run_parser(
        crate::lexer::lex(
            readfilez::read_from_file(std::fs::File::open(filename))?.as_slice(),
            escc,
        ),
        escc,
        pass_escc,
    );
    ret
}
