extern crate readfilez;

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

struct LLParser {
    pm: LLParserMode,
    secs: TwoVec<u8>,
    escc: u8,
    pass_escc: bool,
}

impl LLParser {
    fn new(escc: u8, pass_escc: bool) -> Self {
        Self {
            pm: LLParserMode::Normal,
            secs: TwoVec::new(),
            escc,
            pass_escc,
        }
    }

    // we need to use (&mut self) because we can't invalidate self
    // without making run_parser much more complex
    fn finish(&mut self) -> std::io::Result<Vec<Vec<u8>>> {
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

    fn feed(&mut self, input: &[u8]) -> Vec<Vec<u8>> {
        // we should be able to parse non-utf8 input,
        // as long as the parts starting with ESCC '(' ( and ending with ')')
        // are valid utf8
        for &i in input {
            match self.pm {
                Normal => {
                    if i == self.escc {
                        self.pm = GroupN(0);
                    } else {
                        match i {
                            // 40 = '(' // 41 = ')'
                            40 => {
                                self.pm = GroupN(1);
                                self.secs.up_push();
                            }
                            41 => {
                                // !'('
                                eprintln!("crulz: WARNING: unexpected unbalanced ')'");
                            }
                            _ => {}
                        }
                        self.secs.push(i);
                    }
                }
                GroupN(0) => {
                    // we are at the beginning of a command (after '\\'), expect '('
                    match i {
                        // '(' // !')'
                        40 => {
                            self.pm = GroupN(1);
                            self.secs.up_push();
                            self.secs.push(self.escc);
                        }
                        _ => {
                            self.pm = Normal;
                            if self.pass_escc {
                                self.secs.push(self.escc);
                            }
                        }
                    }
                    self.secs.push(i);
                }
                GroupN(x) => {
                    self.secs.push(i);
                    match i {
                        40 => self.pm.incr(),
                        41 => {
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

pub trait IsSpace {
    fn is_space(self) -> bool;
}

impl IsSpace for u8 {
    fn is_space(self) -> bool {
        match self {
            9 | 10 | 11 | 12 | 13 | 32 => true,
            _ => false,
        }
    }
}

pub type Sections = Vec<(bool, Vec<u8>)>;
type ParserHelperFn<'a> = Box<dyn FnOnce(&mut LLParser) -> Vec<Vec<u8>> + 'a>;

fn run_parser<'a>(escc: u8, pass_escc: bool, fnx: ParserHelperFn<'a>) -> Sections {
    let mut parser = LLParser::new(escc, pass_escc);
    let cls: Vec<ParserHelperFn<'_>> = vec![
        fnx,
        Box::new(|parser| parser.finish().expect("unexpected EOF")),
    ];
    cls.into_iter()
        .map(|fnx| fnx(&mut parser))
        .flatten()
        .map(|section| {
            assert!(!section.is_empty());
            if section[0] == escc && section.len() > 2 && section[1] == 40 && *section.last().unwrap() == 41 {
                (true, section[2..section.len() - 1].to_vec())
            } else {
                (false, section)
            }
        })
        .collect()
}

pub fn parse_whole(input: &[u8], escc: u8, pass_escc: bool) -> Sections {
    run_parser(escc, pass_escc, Box::new(|parser| parser.feed(input)))
}

pub fn file2secs(filename: String, escc: u8, pass_escc: bool) -> Sections {
    run_parser(
        escc,
        pass_escc,
        Box::new(|parser| {
            readfilez::ContinuableFile::new(
                std::fs::File::open(filename).expect("unable to open file"),
            )
            .to_chunks(readfilez::LengthSpec::new(None, true))
            .map(|i| parser.feed(i.expect("unable to read file").get_slice()))
            .flatten()
            .collect()
        }),
    )
}
