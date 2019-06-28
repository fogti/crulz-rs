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

enum ParserMode {
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

pub struct Parser {
    pm: ParserMode,
    secs: TwoVec<u8>,
    escc: u8,
}

impl Parser {
    pub fn new(escc: u8) -> Self {
        Self {
            pm: ParserMode::Normal,
            secs: TwoVec::new(),
            escc,
        }
    }

    pub fn finish(&mut self) -> std::io::Result<Vec<Vec<u8>>> {
        use std::io;
        self.secs.up_push();
        if let ParserMode::Normal = self.pm {
            Ok(std::mem::replace(&mut self.secs.parts, vec![]))
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Parser::finish",
            ))
        }
    }

    pub fn feed(&mut self, input: &[u8]) -> Vec<Vec<u8>> {
        // we should be able to parse non-utf8 input,
        // as long as the parts starting with ESCC '(' ( and ending with ')')
        // are valid utf8
        for &i in input.iter() {
            use ParserMode::*;
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
    {
        let mut parser = Parser::new(escc);
        let sectr = |section: Vec<u8>| {
            assert!(!section.is_empty());
            if *section.first().unwrap() == escc {
                (true, section[2..section.len() - 1].to_vec())
            } else {
                (false, section)
            }
        };
        type SlcfnT = Box<dyn FnOnce(&mut Parser) -> Vec<Vec<u8>>>;
        let per_slicefn = |fnx: SlcfnT| {
            let parts: Sections = fnx(&mut parser).into_iter().map(sectr).collect();
            parts
        };
        let fsf = std::fs::File::open(filename);
        let cls: Vec<SlcfnT> = vec![
            Box::new(|parser| {
                parser.feed(
                    readfilez::read_from_file(fsf)
                        .expect("unable to open file")
                        .get_slice(),
                )
            }),
            Box::new(|parser| parser.finish().expect("unexpected EOF")),
        ];

        cls.into_iter().map(per_slicefn).flatten().collect()
    }
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
