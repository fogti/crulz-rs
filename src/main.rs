extern crate readfilez;

fn errmsg(s: &str) {
    use std::io::Write;
    let res = writeln!(std::io::stderr(), "crulz: ERROR: {}", s);
    if let Err(_) = res {
        std::process::exit(2);
    } else {
        std::process::exit(1);
    }
}

enum LLParserMode {
    Normal,
    CmdN(u32),
}

struct TwoVec<T> {
    pub parts: Vec<Vec<T>>,
    last: Vec<T>,
}

impl<T> TwoVec<T> {
    fn new() -> Self {
        Self {
            parts: vec![],
            last: vec![],
        }
    }

    fn up_push(&mut self) {
        let tmp = std::mem::replace(&mut self.last, vec![]);
        if !tmp.is_empty() {
            self.parts.push(tmp);
        }
    }

    fn push(&mut self, x: T) {
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

type Sections = Vec<(bool, Vec<u8>)>;

fn file2secs(filename: &str, escc: u8) -> Sections {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut escc = '\\' as u8;

    if args.len() > 2 && &args[1] == "-e" {
        let a2 = args[2].as_bytes();
        if a2.len() != 1 {
            errmsg("invalid escc argument");
        }
        escc = a2[0];
    }

    if args.len() < 2 {
        println!("USAGE: crulz [-e ESCC] INPUT");
        std::process::exit(1);
    }

    let parts = file2secs(&args[1], escc);

    for i in &parts {
        let (is_cmdeval, section) = i;
        println!("{} : {:?}", is_cmdeval, section);
    }
}
