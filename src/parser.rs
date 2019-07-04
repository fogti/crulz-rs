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

#[derive(Copy, Clone, Debug, PartialEq)]
enum LLCPR {
    Normal,
    Grouped,
    Escaped,
}

impl std::default::Default for LLCPR {
    fn default() -> Self {
        LLCPR::Normal
    }
}

fn llparse(input: &[LLT], escc: u8, pass_escc: bool) -> std::io::Result<Vec<Vec<LLT>>> {
    let mut pm = LLParserMode::Normal;
    let mut secs = TwoVec::<LLT>::new();

    // we should be able to parse non-utf8 input,
    // as long as the parts starting with ESCC '(' ( and ending with ')')
    // are valid utf8

    let mut is_escaped = false;
    let mut flipp = false;
    let mut clret = LLCPR::Normal;
    let mut nesting: usize = 0;

    use std::time::Instant;
    use rayon::prelude::*;

    let now = Instant::now();
    let classified_ret = classify_as_vec(input.into_iter().map(|i| *i), |i| {
        match nesting {
            0 => {
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
                    nesting += 1;
                    flipp ^= true;
                }
            }
            1 if is_escaped => { // escaped
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
                        nesting -= 1;
                        clret = LLCPR::Normal;
                        let old_flipp = flipp;
                        flipp ^= true;
                        return (old_flipp, LLCPR::Escaped);
                    }
                }
            }
            _ => { // grouped
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
        }
        return (flipp, clret);
    }).into_par_iter()
    .map(|((_, d), i)| {
        match d {
            LLCPR::Escaped if !pass_escc && i.len() == 2 => {
                if let LowerLexerToken::Paren(true) = i[1] {
                    panic!("crulz: INTERNAL ERROR: unbalanced opening '('"); // ')'
                }
                vec![i[1]]
            }
            _ => i,
        }
    })
    .collect::<Vec<Vec<LLT>>>();
    println!("clf : {} µs", now.elapsed().as_micros());

    let now = Instant::now();
    for &i in input {
        match pm {
            Normal => {
                if i.is_escape() {
                    pm = GroupN(0);
                } else {
                    match i {
                        LowerLexerToken::Paren(true) => {
                            pm = GroupN(1);
                            secs.up_push();
                        }
                        LowerLexerToken::Paren(false) => {
                            panic!("crulz: ERROR: unexpected unbalanced ')'");
                        }
                        _ => {}
                    }
                    secs.push(i);
                }
            }
            GroupN(0) => {
                // we are at the beginning of a command (after '\\'), expect '('
                match i {
                    // '(' // !')'
                    LowerLexerToken::Paren(true) => {
                        pm = GroupN(1);
                        secs.up_push();
                        secs.push(LowerLexerToken::Escape(escc));
                        secs.push(i);
                    }
                    _ => {
                        pm = Normal;
                        if pass_escc {
                            secs.push(LowerLexerToken::Escape(escc));
                        }
                        secs.push(i);
                        secs.up_push();
                    }
                }
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

    let ret = secs.finish();
    println!("old : {} µs", now.elapsed().as_micros());

    if ret != classified_ret {
        println!("=== parser return values differ ===");
        println!("{:#?}", classified_ret);
        println!("=== old parser result ===");
        println!("{:#?}", ret);
        println!("=== ---- ===");
    }

    if LLParserMode::Normal == pm {
        Ok(ret)
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
    let llparsed = llparse(input, escc, pass_escc)?.into_iter().map(|section| {
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
