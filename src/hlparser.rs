use crate::llparser::Sections;

#[derive(Clone, Debug)]
pub enum ASTNode {
    Space(Vec<u8>),
    Constant(Vec<u8>),
    Grouped(Box<Vec<ASTNode>>),
    CmdEval(String, Box<Vec<ASTNode>>),
}

pub trait ToU8Vc {
    fn to_u8v(self, escc: u8) -> Vec<u8>;
}

impl ToU8Vc for ASTNode {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        use ASTNode::*;
        match self {
            Space(x) => x,
            Constant(x) => x,
            Grouped(elems) => {
                let mut inner = elems.to_u8v(escc);
                let mut ret = Vec::<u8>::with_capacity(2 + inner.len());
                ret.push(40);
                ret.append(&mut inner);
                ret.push(41);
                ret
            }
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
            assert!(!section.is_empty());
            let slen = section.len();
            use crate::llparser::{parse_whole, IsSpace};
            if is_cmdeval {
                let first_space = section.iter().position(|&x| x.is_space());
                let rest = match first_space {
                    None => &[],
                    Some(x) => &section[x + 1..],
                };

                top.push(ASTNode::CmdEval(
                    std::str::from_utf8(&section[0..first_space.unwrap_or(slen)])
                        .expect("got non-utf8 symbol")
                        .to_owned(),
                    Box::new(crossparse!(parse_whole, rest, escc)),
                ));
            } else if *section.first().unwrap() == 40 && *section.last().unwrap() == 41 {
                top.push(ASTNode::Grouped(Box::new(crossparse!(
                    parse_whole,
                    &section[1..slen - 1],
                    escc
                ))));
            } else {
                top.extend(
                    crate::sharpen::classify_bstr(
                        section,
                        |_ocl, i| i.is_space(),
                        false,
                    )
                    .into_iter()
                    .map(|i| {
                        let (ccl, x) = i;
                        if ccl { ASTNode::Space(x) } else { ASTNode::Constant(x) }
                    }),
                );
            }
        }

        top
    }
}
