mod llparser;
use llparser::file2secs;

pub fn errmsg(s: &str) {
    use std::io::Write;
    let res = writeln!(std::io::stderr(), "crulz: ERROR: {}", s);
    if let Err(_) = res {
        std::process::exit(2);
    } else {
        std::process::exit(1);
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
