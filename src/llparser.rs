extern crate readfilez;

enum LLParserMode {
    Normal,
    CmdN(u32),
}

pub struct TwoVec<T> {
    pub parts: Vec<Vec<T>>,
    last: Vec<T>,
}

impl<T> TwoVec<T> {
    pub fn new() -> Self {
        Self {
            parts: vec![],
            last: vec![],
        }
    }

    pub fn finish(mut self) -> Vec<Vec<T>> {
        self.up_push();
        self.parts
    }

    pub fn up_push(&mut self) {
        let tmp = std::mem::replace(&mut self.last, vec![]);
        if !tmp.is_empty() {
            self.parts.push(tmp);
        }
    }

    pub fn push(&mut self, x: T) {
        self.last.push(x);
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
        self.secs.up_push();
        if let LLParserMode::Normal = self.pm {
            Ok(std::mem::replace(&mut self.secs.parts, vec![]))
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
            match self.pm {
                Normal => {
                    if i == self.escc {
                        self.pm = CmdN(0);
                    } else {
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
                CmdN(x) => {
                    self.secs.push(i);
                    match i {
                        40 => self.pm = CmdN(x + 1),
                        41 => self.pm = if x == 1 { Normal } else { CmdN(x - 1) },
                        _ => {}
                    }
                    if let Normal = self.pm {
                        self.secs.up_push();
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

impl IsSpace for char {
    fn is_space(self) -> bool {
        match self {
            '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ' => true,
            _ => false,
        }
    }
}

pub type Sections = Vec<(bool, Vec<u8>)>;

pub fn parse_whole(input: &[u8], escc: u8) -> Sections {
    let mut parser = LLParser::new(escc);
    let cls: Vec<Box<dyn FnOnce(&mut LLParser) -> Vec<Vec<u8>>>> = vec![
        Box::new(|parser| parser.feed(input)),
        Box::new(|parser| parser.finish().expect("unexpected EOF")),
    ];
    cls.into_iter()
        .map(|fnx| {
            fnx(&mut parser).into_iter().map(|section: Vec<u8>| {
                assert!(!section.is_empty());
                if *section.first().unwrap() == escc {
                    (true, section[2..section.len() - 1].to_vec())
                } else {
                    (false, section)
                }
            })
        })
        .flatten()
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
        .map(|fnx| {
            fnx(&mut parser).into_iter().map(|section: Vec<u8>| {
                assert!(!section.is_empty());
                if *section.first().unwrap() == escc {
                    (true, section[2..section.len() - 1].to_vec())
                } else {
                    (false, section)
                }
            })
        })
        .flatten()
        .collect()
}
