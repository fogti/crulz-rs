extern crate readfilez;

use crate::sharpen::*;

#[derive(Clone, Copy)]
enum LLParserMode {
    Normal,
    GroupN(u32),
    CmdN(u32),
}

impl LLParserMode {
    fn incr(&mut self) {
        use LLParserMode::*;
        let res = match &self {
            GroupN(x) => GroupN(x + 1),
            CmdN(x) => CmdN(x + 1),
            _ => *self,
        };
        *self = res;
    }
    fn decr(&mut self) {
        use LLParserMode::*;
        let res = match &self {
            GroupN(x) => GroupN(x - 1),
            CmdN(x) => CmdN(x - 1),
            _ => *self,
        };
        *self = res;
    }
}

pub struct LLParser {
    pm: LLParserMode,
    secs: TwoVec<u8>,
    escc: u8,
}

impl LLParser {
    pub fn new(escc: u8) -> Self {
        Self {
            pm: LLParserMode::Normal,
            secs: TwoVec::new(),
            escc,
        }
    }

    // we need to use (&mut self) because we can't invalidate self
    // without making parse_whole and file2secs much more complex
    pub fn finish(&mut self) -> std::io::Result<Vec<Vec<u8>>> {
        use std::io;
        if let LLParserMode::Normal = self.pm {
            Ok(self.secs.finish())
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LLParser::finish",
            ))
        }
    }

    pub fn feed(&mut self, input: &[u8]) -> Vec<Vec<u8>> {
        // we should be able to parse non-utf8 input,
        // as long as the parts starting with ESCC '(' ( and ending with ')')
        // are valid utf8
        for &i in input.iter() {
            use LLParserMode::*;
            let mut r2normal = false;
            match self.pm {
                Normal | GroupN(0) => {
                    if let GroupN(0) = self.pm {
                        self.pm = Normal;
                        self.secs.up_push();
                    }
                    if i == self.escc {
                        self.pm = CmdN(0);
                    } else {
                        match i {
                            // 40 = '(' // 41 = ')'
                            40 => {
                                self.pm = GroupN(1);
                                self.secs.up_push();
                            }
                            41 => {
                                use std::io::Write; // !'('
                                let _ = writeln!(
                                    std::io::stderr(),
                                    "crulz: WARNING: unexpected unbalanced ')'"
                                );
                            }
                            _ => {}
                        }
                        self.secs.push(i);
                    }
                }
                CmdN(0) => {
                    match i {
                        // '(' // !')'
                        40 => {
                            self.pm = CmdN(1);
                            self.secs.up_push();
                            self.secs.push(self.escc);
                        }
                        _ => {
                            self.pm = Normal;
                        }
                    }
                    self.secs.push(i);
                }
                CmdN(x) | GroupN(x) => {
                    self.secs.push(i);
                    match i {
                        40 => self.pm.incr(),
                        41 => {
                            self.pm.decr();
                            if x == 1 {
                                r2normal = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if r2normal {
                self.pm = Normal;
                self.secs.up_push();
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

impl IsSpace for char {
    fn is_space(self) -> bool {
        match self {
            '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ' => true,
            _ => false,
        }
    }
}

pub type Sections = Vec<(bool, Vec<u8>)>;

fn remap_section(section: Vec<u8>, escc: u8) -> (bool, Vec<u8>) {
    assert!(!section.is_empty());
    if *section.first().unwrap() == escc {
        (true, section[2..section.len() - 1].to_vec())
    } else {
        (false, section)
    }
}

pub fn parse_whole(input: &[u8], escc: u8) -> Sections {
    let mut parser = LLParser::new(escc);
    let cls: Vec<Box<dyn FnOnce(&mut LLParser) -> Vec<Vec<u8>>>> = vec![
        Box::new(|parser| parser.feed(input)),
        Box::new(|parser| parser.finish().expect("unexpected EOF")),
    ];
    cls.into_iter()
        .map(|fnx| fnx(&mut parser))
        .flatten()
        .map(|section| remap_section(section, escc))
        .collect()
}

pub fn file2secs(filename: &str, escc: u8) -> Sections {
    let mut parser = LLParser::new(escc);
    let filename = filename.to_owned();
    let cls: Vec<Box<dyn FnOnce(&mut LLParser) -> Vec<Vec<u8>>>> = vec![
        Box::new(|parser| {
            readfilez::ContinuableFile::new(
                std::fs::File::open(filename).expect("unable to open file"),
            )
            .to_chunks(readfilez::LengthSpec::new(None, true))
            .map(|i| parser.feed(i.expect("unable to read file").get_slice()))
            .flatten()
            .collect()
        }),
        Box::new(|parser| parser.finish().expect("unexpected EOF")),
    ];
    cls.into_iter()
        .map(|fnx| fnx(&mut parser))
        .flatten()
        .map(|section| remap_section(section, escc))
        .collect()
}
