use crate::llparser::Sections;

#[derive(Clone, Debug, PartialEq)]
pub enum ASTNode {
    NullNode,
    Space(Vec<u8>),
    Constant(Vec<u8>),

    /// Grouped: is_strict, elems
    /// loose groups are created while replacing patterns
    Grouped(bool, Box<Vec<ASTNode>>),

    CmdEval(String, Box<Vec<ASTNode>>),
}

pub trait MangleAST {
    fn to_u8v(self, escc: u8) -> Vec<u8>;

    /// helper for MangleAST::simplify
    fn get_complexity(&self) -> usize;

    /// this cleanup up the AST
    fn simplify(&mut self);

    /// this replace function works on byte-basis and honours ASTNode boundaries
    fn replace(&mut self, from: &[u8], to: &[ASTNode]);
}

// helper for MangleAST::simplify
#[derive(Copy, Clone, Debug, PartialEq)]
enum ASTNodeClass {
    NullNode,
    Space,
    Constant,
    Grouped(bool),
    CmdEval,
}

impl ASTNode {
    fn get_class(&self) -> ASTNodeClass {
        use ASTNodeClass::*;
        match &self {
            ASTNode::NullNode => NullNode,
            // allow early reduction
            ASTNode::Grouped(false, x) if x.is_empty() => NullNode,
            ASTNode::Space(_) => Space,
            ASTNode::Constant(_) => Constant,
            ASTNode::Grouped(s, _) => Grouped(*s),
            ASTNode::CmdEval(_, _) => CmdEval,
        }
    }
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

    fn get_complexity(&self) -> usize {
        use ASTNode::*;
        match &self {
            NullNode => 0,
            Constant(x) | Space(x) => 1 + x.len(),
            Grouped(_, x) => 2 + x.len(),
            CmdEval(cmd, x) => 1 + cmd.len() + x.len(),
        }
    }

    fn simplify(&mut self) {
        use ASTNode::*;
        let mut cplx = self.get_complexity();
        loop {
            let mut rep = NullNode;
            match &self {
                Grouped(false, x) => {
                    let mut y = x.clone();
                    y.simplify();
                    rep = match y.len() {
                        0 => NullNode,
                        1 => y.first().unwrap().clone(),
                        _ => Grouped(false, y),
                    };
                }
                _ => return,
            }
            *self = rep;
            let new_cplx = self.get_complexity();
            if new_cplx >= cplx {
                break;
            }
            cplx = new_cplx;
        }
    }

    fn replace(&mut self, from: &[u8], to: &[ASTNode]) {
        let flen = from.len();
        if flen == 0 {
            return;
        }
        use rayon::prelude::*;
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
                        .into_par_iter()
                        .map(|(d, i)| {
                            // replace matches with 'None'
                            use boolinator::Boolinator;
                            (!(d && i.len() == flen)).as_some(i)
                        })
                        .collect::<Vec<_>>()
                        .classify(|_, i| i.is_some(), true)
                        .into_par_iter()
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
            _ => *self = rep,
        }
    }
}

impl MangleAST for Vec<ASTNode> {
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        self.into_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
    fn get_complexity(&self) -> usize {
        use rayon::prelude::*;
        self.par_iter().map(|i| i.get_complexity()).sum()
    }
    fn simplify(&mut self) {
        use crate::sharpen::Classify;
        use rayon::prelude::*;
        self.par_iter_mut().for_each(|i| i.simplify());
        *self = self
            .into_iter()
            .filter(|i| {
                use ASTNode::*;
                match *i {
                    NullNode => false,
                    _ => true,
                }
            })
            .classify(|_, i| i.get_class(), ASTNodeClass::NullNode)
            .into_par_iter()
            .map(|(d, i)| {
                use ASTNode::*;
                match d {
                    ASTNodeClass::NullNode => vec![NullNode],
                    _ if i.len() < 2 => i,
                    ASTNodeClass::Space => vec![Space(
                        i.into_iter()
                            .map(|j| {
                                if let Space(x) = j {
                                    x
                                } else {
                                    unreachable!();
                                }
                            })
                            .flatten()
                            .collect(),
                    )],
                    ASTNodeClass::Constant => vec![Constant(
                        i.into_iter()
                            .map(|j| {
                                if let Constant(x) = j {
                                    x
                                } else {
                                    unreachable!();
                                }
                            })
                            .flatten()
                            .collect(),
                    )],
                    ASTNodeClass::Grouped(false) => vec![Grouped(
                        false,
                        Box::new(
                            i.into_iter()
                                .map(|j| {
                                    if let Grouped(_, x) = j {
                                        *x
                                    } else {
                                        unreachable!();
                                    }
                                })
                                .flatten()
                                .collect(),
                        ),
                    )],
                    _ => i,
                }
            })
            .flatten()
            .collect::<Vec<ASTNode>>();
    }
    fn replace(&mut self, from: &[u8], to: &[ASTNode]) {
        if from.len() == 0 {
            return;
        }
        use rayon::prelude::*;
        self.par_iter_mut().for_each(|i| i.replace(from, to));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace() {
        use ASTNode::*;
        let mut ast = Grouped(false, Box::new(vec![Constant(vec![0, 1, 2, 3])]));
        ast.replace(
            &vec![1, 2],
            &[Grouped(false, Box::new(vec![Constant(vec![4])]))],
        );
        assert_eq!(
            ast,
            Grouped(
                false,
                Box::new(vec![Grouped(
                    false,
                    Box::new(vec![
                        Constant(vec![0]),
                        Grouped(
                            false,
                            Box::new(vec![Grouped(false, Box::new(vec![Constant(vec![4])]))])
                        ),
                        Constant(vec![3])
                    ])
                )])
            )
        );
    }

    #[test]
    fn test_simplify() {
        use ASTNode::*;
        let mut ast = Grouped(
            false,
            Box::new(vec![Grouped(
                false,
                Box::new(vec![
                    Constant(vec![0]),
                    Grouped(
                        false,
                        Box::new(vec![Grouped(false, Box::new(vec![Constant(vec![4])]))]),
                    ),
                    Constant(vec![3]),
                ]),
            )]),
        );
        ast.simplify();
        assert_eq!(ast, Constant(vec![0, 4, 3]));
    }
}
