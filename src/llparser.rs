use crate::sharpen::*;

#[derive(Clone, Copy)]
enum LLParserMode {
    Normal,
    GroupN(u32),
}

use crate::llparser::LLParserMode::*;

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

struct LLParser {
    pm: LLParserMode,
    secs: TwoVec<LLT>,
    prev: Option<LLT>,
    pass_escc: bool,
}

impl LLParser {
    fn new(pass_escc: bool) -> Self {
        Self {
            pm: LLParserMode::Normal,
            secs: TwoVec::new(),
            prev: None,
            pass_escc,
        }
    }

    // we need to use (&mut self) because we can't invalidate self
    // without making run_parser much more complex
    fn finish(&mut self) -> std::io::Result<Vec<Vec<LLT>>> {
        if let LLParserMode::Normal = self.pm {
            Ok(self.secs.finish())
        } else {
            use std::io;
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LLParser::finish",
            ))
        }
    }

    fn feed(&mut self, input: &[LLT]) -> Vec<Vec<LLT>> {
        // we should be able to parse non-utf8 input,
        // as long as the parts starting with ESCC '(' ( and ending with ')')
        // are valid utf8
        for &i in input {
            match self.pm {
                Normal => {
                    if i.is_escape() {
                        self.prev = Some(i);
                        self.pm = GroupN(0);
                    } else {
                        match i {
                            LowerLexerToken::Paren(true) => {
                                self.pm = GroupN(1);
                                self.secs.up_push();
                            }
                            LowerLexerToken::Paren(false) => {
                                eprintln!("crulz: WARNING: unexpected unbalanced ')'");
                            }
                            _ => {}
                        }
                        self.secs.push(i);
                        self.prev = None;
                    }
                }
                GroupN(0) => {
                    // we are at the beginning of a command (after '\\'), expect '('
                    match i {
                        // '(' // !')'
                        LowerLexerToken::Paren(true) => {
                            self.pm = GroupN(1);
                            self.secs.up_push();
                            self.secs.push(self.prev.take().unwrap());
                        }
                        _ => {
                            self.pm = Normal;
                            if self.pass_escc {
                                self.secs.push(self.prev.take().unwrap());
                            } else {
                                self.prev = None;
                            }
                        }
                    }
                    self.secs.push(i);
                }
                GroupN(x) => {
                    self.secs.push(i);
                    match i {
                        LowerLexerToken::Paren(true) => self.pm.incr(),
                        LowerLexerToken::Paren(false) => {
                            if x == 1 {
                                self.pm = Normal;
                                self.secs.up_push();
                            } else {
                                self.pm.decr();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        std::mem::replace(&mut self.secs.parts, vec![])
    }
}

pub enum SectionType {
    Normal,
    Grouped,
    CmdEval,
}

pub type Sections = Vec<(SectionType, Vec<LLT>)>;
type ParserHelperFn<'a> = Box<dyn FnOnce(&mut LLParser) -> Vec<Vec<LLT>> + 'a>;

fn run_parser<'a>(pass_escc: bool, fnx: ParserHelperFn<'a>) -> Sections {
    let mut parser = LLParser::new(pass_escc);
    let cls: Vec<ParserHelperFn<'_>> = vec![
        fnx,
        Box::new(|parser| parser.finish().expect("unexpected EOF")),
    ];
    cls.into_iter()
        .map(|fnx| fnx(&mut parser))
        .flatten()
        .map(|section| {
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
        })
        .collect()
}

pub fn parse_lexed(input: &[LLT], pass_escc: bool) -> Sections {
    run_parser(pass_escc, Box::new(|parser| parser.feed(input)))
}

pub fn file2secs(filename: String, escc: u8, pass_escc: bool) -> Sections {
    run_parser(
        pass_escc,
        Box::new(|parser| {
            readfilez::ContinuableFile::new(
                std::fs::File::open(filename).expect("unable to open file"),
            )
            .to_chunks(readfilez::LengthSpec::new(None, true))
            .map(|i| {
                parser.feed(&crate::lexer::lex(
                    i.expect("unable to read file").get_slice(),
                    escc,
                ))
            })
            .flatten()
            .collect()
        }),
    )
}

use crate::ast::*;

impl ToAST for Sections {
    fn to_ast(self, escc: u8, pass_escc: bool) -> VAN {
        let mut top = VAN::new();

        for (stype, section) in self {
            assert!(!section.is_empty());
            use crate::ast::ASTNode::*;
            use rayon::prelude::*;
            match stype {
                SectionType::CmdEval => {
                    let first_space = section.iter().position(|&x| x.is_space());
                    let rest = first_space.map(|x| &section[x + 1..]).unwrap_or(&[]);

                    top.push(CmdEval(
                        std::str::from_utf8(
                            &section[0..first_space.unwrap_or_else(|| section.len())]
                                .iter()
                                .map(std::convert::Into::<u8>::into)
                                .collect::<Vec<_>>(),
                        )
                        .expect("got non-utf8 symbol")
                        .to_owned(),
                        parse_lexed(rest, pass_escc).to_ast(escc, pass_escc),
                    ));
                }
                SectionType::Grouped => {
                    top.push(Grouped(
                        true,
                        parse_lexed(&section, pass_escc).to_ast(escc, pass_escc),
                    ));
                }
                SectionType::Normal => {
                    top.par_extend(
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

        top
    }
}
