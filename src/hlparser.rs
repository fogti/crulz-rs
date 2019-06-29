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
                let mut rest = args.to_u8v(escc);
                let mut ret = Vec::<u8>::with_capacity(4 + cmd.len() + rest.len());
                ret.push(escc);
                ret.push(40);
                ret.extend_from_slice(cmd.as_bytes());
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
    ($fn:path, $input:expr, $escc:ident) => {{
        // we don't want to import this in every file using this macro
        // but we use it in this file too, and want to suppress the
        // warning about that
        #[allow(unused_imports)]
        use crate::hlparser::ToAST;
        $fn($input, $escc).to_ast($escc)
    }};
}

pub trait ToAST {
    fn to_ast(self, escc: u8) -> Vec<ASTNode>;
}

impl ToAST for Sections {
    fn to_ast(self, escc: u8) -> Vec<ASTNode> {
        let mut top = Vec::<ASTNode>::new();

        for i in self {
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
                top.extend(twv.finish().into_iter().map(|i| {
                    if i.len() == 1 {
                        let x = *i.first().unwrap();
                        if x.is_space() {
                            return ASTNode::Space(x);
                        }
                    }
                    ASTNode::Constant(i)
                }));
            }
        }

        top
    }
}
