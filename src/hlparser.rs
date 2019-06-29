use crate::llparser::Sections;

#[derive(Clone, Debug)]
pub enum ASTNode {
    Space(u8),
    Constant(Vec<u8>),
    CmdEval(String, Box<Vec<ASTNode>>),
}

pub trait ToU8Vc {
    fn to_u8v(self, escc: u8) -> Vec<u8>;
}

impl ToU8Vc for ASTNode {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        use ASTNode::*;
        match self {
            Space(x) => vec![x],
            Constant(x) => x,
            CmdEval(cmd, args) => {
                let mut ret = Vec::<u8>::with_capacity(4 + cmd.len());
                ret.push(escc);
                ret.push(40);
                ret.extend_from_slice(cmd.as_bytes());
                let mut rest = args.to_u8v(escc);
                if !rest.is_empty() {
                    ret.push(32);
                    ret.append(&mut rest);
                }
                ret.push(41);
                ret
            }
        }
    }
}

impl ToU8Vc for Vec<ASTNode> {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        self.into_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
}

macro_rules! crossparse {
    ($fn:path, $input:expr, $escc:ident) => {
        tr_secs($fn($input, $escc), $escc)
    };
}

pub fn tr_secs(parts: Sections, escc: u8) -> Vec<ASTNode> {
    let mut top = Vec::<ASTNode>::new();

    for i in parts {
        let (is_cmdeval, section) = i;
        use crate::llparser::{parse_whole, IsSpace, TwoVec};
        if is_cmdeval {
            let first_space = section.iter().position(|&x| x.is_space());
            let rest = match first_space {
                None => &[],
                Some(x) => &section[x + 1..],
            };

            top.push(ASTNode::CmdEval(
                std::str::from_utf8(&section[0..first_space.unwrap_or(section.len())])
                    .expect("got non-utf8 symbol")
                    .to_owned(),
                Box::new(crossparse!(parse_whole, rest, escc)),
            ));
        } else {
            let mut twv = TwoVec::<u8>::new();
            for i in section {
                if i.is_space() {
                    twv.up_push();
                    twv.push(i);
                    twv.up_push();
                } else {
                    twv.push(i);
                }
            }
            for i in twv.finish() {
                top.push(if i.len() == 1 && i.first().unwrap().is_space() {
                    ASTNode::Space(*i.first().unwrap())
                } else {
                    ASTNode::Constant(i)
                })
            }
        }
    }

    top
}
