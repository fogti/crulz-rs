use crate::sharpen::*;

#[derive(Clone, Copy, PartialEq)]
enum LLParserMode {
    Normal,
    GroupN(u32),
}

use self::LLParserMode::*;

impl LLParserMode {
    fn incr(mut self: &mut Self) {
        if let GroupN(ref mut x) = &mut self {
            *x += 1;
        }
    }
    fn decr(mut self: &mut Self) {
        if let GroupN(ref mut x) = &mut self {
            *x -= 1;
        }
    }
}

use crate::lexer::LowerLexerToken;
type LLT = LowerLexerToken;

fn llparse(pass_escc: bool, input: &[LLT]) -> std::io::Result<Vec<Vec<LLT>>> {
    let mut pm = LLParserMode::Normal;
    let mut secs = TwoVec::<LLT>::new();
    let mut prev = Option::<LLT>::None;

    // we should be able to parse non-utf8 input,
    // as long as the parts starting with ESCC '(' ( and ending with ')')
    // are valid utf8
    for &i in input {
        match pm {
            Normal => {
                if i.is_escape() {
                    prev = Some(i);
                    pm = GroupN(0);
                } else {
                    match i {
                        LowerLexerToken::Paren(true) => {
                            pm = GroupN(1);
                            secs.up_push();
                        }
                        LowerLexerToken::Paren(false) => {
                            panic!("crulz: WARNING: unexpected unbalanced ')'");
                        }
                        _ => {}
                    }
                    secs.push(i);
                    prev = None;
                }
            }
            GroupN(0) => {
                // we are at the beginning of a command (after '\\'), expect '('
                match i {
                    // '(' // !')'
                    LowerLexerToken::Paren(true) => {
                        pm = GroupN(1);
                        secs.up_push();
                        secs.push(prev.take().unwrap());
                    }
                    _ => {
                        pm = Normal;
                        if pass_escc {
                            secs.push(prev.take().unwrap());
                        } else {
                            prev = None;
                        }
                    }
                }
                secs.push(i);
            }
            GroupN(x) => {
                secs.push(i);
                match i {
                    LowerLexerToken::Paren(true) => pm.incr(),
                    LowerLexerToken::Paren(false) => {
                        if x == 1 {
                            pm = Normal;
                            secs.up_push();
                        } else {
                            pm.decr();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if LLParserMode::Normal == pm {
        Ok(secs.finish())
    } else {
        use std::io;
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "LLParser::finish",
        ))
    }
}

pub enum SectionType {
    Normal,
    Grouped,
    CmdEval,
}

use crate::ast::VAN;

type ParserResult = Result<VAN, failure::Error>;

fn run_parser(input: &[LLT], escc: u8, pass_escc: bool) -> ParserResult {
    let llparsed = llparse(pass_escc, input)?.into_iter().map(|section| {
        assert!(!section.is_empty());
        if section[0].is_escape()
            && section.len() > 2
            && section[1] == LowerLexerToken::Paren(true)
            && *section.last().unwrap() == LowerLexerToken::Paren(false)
        {
            (SectionType::CmdEval, section[2..section.len() - 1].to_vec())
        } else if section[0] == LowerLexerToken::Paren(true)
            && *section.last().unwrap() == LowerLexerToken::Paren(false)
        {
            (SectionType::Grouped, section[1..section.len() - 1].to_vec())
        } else {
            (SectionType::Normal, section)
        }
    });

    let mut ret = VAN::new();

    for (stype, section) in llparsed {
        assert!(!section.is_empty());
        use crate::ast::ASTNode::*;
        use rayon::prelude::*;
        match stype {
            SectionType::CmdEval => {
                let first_space = section.iter().position(|&x| x.is_space());
                let rest = first_space.map(|x| &section[x + 1..]).unwrap_or(&[]);

                ret.push(CmdEval(
                    std::str::from_utf8(
                        &section[0..first_space.unwrap_or_else(|| section.len())]
                            .iter()
                            .map(std::convert::Into::<u8>::into)
                            .collect::<Vec<_>>(),
                    )?
                    .to_owned(),
                    run_parser(rest, escc, pass_escc)?,
                ));
            }
            SectionType::Grouped => {
                ret.push(Grouped(true, run_parser(&section, escc, pass_escc)?));
            }
            SectionType::Normal => {
                ret.par_extend(
                    classify_as_vec(section, |i| i.is_space())
                        .into_par_iter()
                        .map(|(ccl, x)| {
                            Constant(
                                !ccl,
                                x.into_iter().map(std::convert::Into::<u8>::into).collect(),
                            )
                        }),
                );
            }
        }
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
