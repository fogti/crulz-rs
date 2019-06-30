use crate::llparser::Sections;

#[derive(Clone, Debug, PartialEq)]
pub enum ASTNode {
    NullNode,
    Space(Vec<u8>),
    Constant(Vec<u8>),
    // Grouped: is_strict, elems
    // loose groups are created while replacing patterns
    Grouped(bool, Box<Vec<ASTNode>>),
    CmdEval(String, Box<Vec<ASTNode>>),
}

pub trait MangleAST {
    fn to_u8v(self, escc: u8) -> Vec<u8>;

    // this replace function works on byte-basis and honours ASTNode boundaries
    fn replace(&mut self, from: &[u8], to: &[ASTNode]);
}

impl MangleAST for ASTNode {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        use ASTNode::*;
        match self {
            NullNode => Vec::new(),
            Space(x) => x,
            Constant(x) => x,
            Grouped(is_strict, elems) => {
                let mut inner = elems.to_u8v(escc);
                let mut ret = Vec::<u8>::with_capacity(2 + inner.len());
                if is_strict {
                    ret.push(40);
                }
                ret.append(&mut inner);
                if is_strict {
                    ret.push(41);
                }
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
    fn replace(&mut self, from: &[u8], to: &[ASTNode]) {
        let flen = from.len();
        if flen == 0 {
            return;
        }
        use ASTNode::*;
        let mut rep = NullNode;
        match self {
            Constant(ref x) => {
                let start = *from.first().unwrap();
                let mut skp: usize = 0;
                use crate::sharpen::Classify;
                rep = Grouped(
                    false,
                    Box::new(
                        x.classify(
                            |d, &i| {
                                if !d {
                                    // from currently not found
                                    if i == start {
                                        skp = 1;
                                        return true;
                                    }
                                } else {
                                    // currently inside from
                                    if skp == flen {
                                        skp = 0;
                                    } else if i == from[skp] {
                                        skp = skp + 1;
                                        return true;
                                    }
                                }
                                false
                            },
                            false,
                        )
                        .into_iter()
                        .map(|(d, i)| {
                            // replace matches with 'None'
                            use boolinator::Boolinator;
                            (!(d && i.len() == flen)).as_some(i)
                        })
                        .collect::<Vec<_>>()
                        .classify(|_, i| i.is_some(), true)
                        .into_iter()
                        .map(|(d, i)| {
                            if d {
                                // 'Some'
                                Constant(
                                    i.into_iter()
                                        .map(|j| j.unwrap_or(Vec::new()))
                                        .flatten()
                                        .collect(),
                                )
                            } else {
                                // 'None'
                                Grouped(
                                    false,
                                    Box::new(
                                        std::iter::repeat(to)
                                            .take(i.len())
                                            .flatten()
                                            .map(|i| i.clone())
                                            .collect::<Vec<_>>(),
                                    ),
                                )
                            }
                        })
                        .collect::<Vec<ASTNode>>(),
                    ),
                );
            }
            Grouped(is_strict, ref mut x) => {
                let mut xt = x.clone();
                xt.replace(from, to);
                *self = Grouped(*is_strict, xt);
                return;
            }
            CmdEval(cmd, ref mut args) => {
                // TODO: mangle cmd
                let mut xt = args.clone();
                xt.replace(from, to);
                *self = CmdEval(cmd.clone(), xt);
                return;
            }
            // we ignore spaces
            _ => return,
        }
        match &rep {
            NullNode => return,
            Grouped(false, x) => {
                *self = match x.len() {
                    0 => NullNode,
                    1 => x.first().unwrap().clone(),
                    _ => rep,
                };
            }
            _ => *self = rep,
        }
    }
}

impl MangleAST for Vec<ASTNode> {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        self.into_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
    fn replace(&mut self, from: &[u8], to: &[ASTNode]) {
        if from.len() == 0 {
            return;
        }
        for i in self.iter_mut() {
            i.replace(from, to);
        }
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
                top.push(ASTNode::Grouped(
                    true,
                    Box::new(crossparse!(parse_whole, &section[1..slen - 1], escc)),
                ));
            } else {
                use crate::sharpen::Classify;
                top.extend(
                    section
                        .classify(|_ocl, i| i.is_space(), false)
                        .into_iter()
                        .map(|i| {
                            let (ccl, x) = i;
                            if ccl {
                                ASTNode::Space(x)
                            } else {
                                ASTNode::Constant(x)
                            }
                        }),
                );
            }
        }

        top
    }
}
