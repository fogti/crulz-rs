use crate::llparser::Sections;
use crate::sharpen::Classify;
use rayon::prelude::*;

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

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

pub trait MangleAST {
    type LiftT;
    // lift the AST one level up (ASTNode -> VAN || VAN -> ASTNode),
    // used as helper for MangleAST::simplify and others
    // to convert to the appropriate datatype
    fn lift_ast(self) -> Self::LiftT;

    fn to_u8v(self, escc: u8) -> Vec<u8>;

    /// helper for MangleAST::simplify
    fn get_complexity(&self) -> usize;

    /// this cleanup up the AST, opposite of two lift_ast invocations
    fn simplify(&mut self);

    /// this replace function works on byte-basis and honours ASTNode boundaries
    fn replace(&mut self, from: &[u8], to: &ASTNode);
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

impl std::default::Default for ASTNodeClass {
    fn default() -> Self {
        ASTNodeClass::NullNode
    }
}

impl ASTNode {
    pub fn get_constant(&self) -> Option<&Vec<u8>> {
        match &self {
            ASTNode::Constant(x) => Some(x),
            _ => None,
        }
    }
}

impl MangleAST for ASTNode {
    type LiftT = Vec<ASTNode>;
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }

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
            Grouped(_, x) => 2 + x.get_complexity(),
            CmdEval(cmd, x) => 1 + cmd.len() + x.get_complexity(),
        }
    }

    fn simplify(&mut self) {
        use ASTNode::*;
        let mut cplx = self.get_complexity();
        loop {
            let rep;
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
                Grouped(true, x) => {
                    let mut y = *x.clone();
                    y.simplify();
                    rep = Grouped(
                        true,
                        Box::new(if y.len() == 1 {
                            match y.first().unwrap() {
                                Grouped(_, x) => *x.clone(),
                                _ => y,
                            }
                        } else {
                            y
                        }),
                    );
                }
                _ => return,
            }
            let new_cplx = rep.get_complexity();
            if new_cplx >= cplx {
                break;
            }
            *self = rep;
            cplx = new_cplx;
        }
    }

    fn replace(&mut self, from: &[u8], to: &ASTNode) {
        let flen = from.len();
        if flen == 0 {
            return;
        }
        use ASTNode::*;
        let rep;
        match self {
            Constant(ref x) => {
                let start = *from.first().unwrap();
                let mut skp: usize = 0;
                rep = x
                    .classify(|d: bool, &i| {
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
                                skp += 1;
                                return true;
                            }
                        }
                        false
                    })
                    .into_par_iter()
                    .map(|(d, i)| {
                        // replace matches with 'None'
                        use boolinator::Boolinator;
                        (!(d && i.len() == flen)).as_some(i)
                    })
                    // TODO: the following line shouldn't be needed, but classify currently needs it
                    .collect::<Vec<_>>()
                    .classify(|_, i| i.is_some())
                    .into_par_iter()
                    .map(|(d, i)| {
                        if d {
                            // 'Some'
                            Constant(
                                i.into_par_iter()
                                    .map(|j| j.unwrap_or(Vec::new()))
                                    .flatten()
                                    .collect(),
                            )
                        } else {
                            // 'None'
                            std::iter::repeat(to)
                                .take(i.len())
                                .map(|i| i.clone())
                                .collect::<Vec<_>>()
                                .lift_ast()
                        }
                    })
                    .collect::<Vec<_>>()
                    .lift_ast();
            }
            Grouped(is_strict, x) => {
                let mut xt = x.clone();
                xt.replace(from, to);
                *self = Grouped(*is_strict, xt);
                return;
            }
            CmdEval(cmd, args) => {
                let mut cmd = cmd.clone();
                // mangle cmd
                if let Constant(to2) = &to {
                    use std::str;
                    if let Ok(from2) = str::from_utf8(from) {
                        if let Ok(to3) = str::from_utf8(&to2) {
                            cmd = cmd.replace(from2, to3);
                        }
                    }
                }

                // mangle args
                let mut args = args.clone();
                args.replace(from, to);
                *self = CmdEval(cmd, args);
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
    type LiftT = ASTNode;
    fn lift_ast(self) -> Self::LiftT {
        ASTNode::Grouped(false, Box::new(self))
    }

    fn to_u8v(self, escc: u8) -> Vec<u8> {
        self.into_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
    fn get_complexity(&self) -> usize {
        self.par_iter().map(|i| i.get_complexity()).sum()
    }
    fn simplify(&mut self) {
        self.par_iter_mut().for_each(|i| i.simplify());
        *self = self
            .into_iter()
            .filter(|i| {
                use ASTNode::*;
                match *i {
                    NullNode => false,
                    Grouped(false, ref x) if x.is_empty() => false,
                    Constant(ref x) if x.is_empty() => false,
                    Space(ref x) if x.is_empty() => false,
                    _ => true,
                }
            })
            .classify(|_, i| {
                use ASTNodeClass::*;
                match &i {
                    ASTNode::Space(_) => Space,
                    ASTNode::Constant(_) => Constant,
                    ASTNode::Grouped(s, _) => Grouped(*s),
                    ASTNode::CmdEval(_, _) => CmdEval,
                    _ => NullNode,
                }
            })
            .into_par_iter()
            .map(|(d, i)| {
                use ASTNode::*;
                match d {
                    ASTNodeClass::NullNode => NullNode.lift_ast(),
                    _ if i.len() < 2 => i,
                    ASTNodeClass::Space => Space(
                        i.into_par_iter()
                            .map(|j| {
                                if let Space(x) = j {
                                    x
                                } else {
                                    unreachable!();
                                }
                            })
                            .flatten()
                            .collect(),
                    )
                    .lift_ast(),
                    ASTNodeClass::Constant => Constant(
                        i.into_par_iter()
                            .map(|j| {
                                if let Constant(x) = j {
                                    x
                                } else {
                                    unreachable!();
                                }
                            })
                            .flatten()
                            .collect(),
                    )
                    .lift_ast(),
                    ASTNodeClass::Grouped(false) => i
                        .into_par_iter()
                        .map(|j| {
                            if let Grouped(_, x) = j {
                                *x
                            } else {
                                unreachable!();
                            }
                        })
                        .flatten()
                        .collect::<Vec<_>>(),
                    _ => i,
                }
            })
            .flatten()
            .collect::<Vec<ASTNode>>();
    }
    fn replace(&mut self, from: &[u8], to: &ASTNode) {
        if from.len() == 0 {
            return;
        }
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
                top.par_extend(
                    section
                        .classify(|_ocl, i| i.is_space())
                        .into_par_iter()
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
        let mut ast = vec![Constant(vec![0, 1, 2, 3])].lift_ast();
        ast.replace(&vec![1, 2], &Constant(vec![4]));
        assert_eq!(
            ast,
            vec![
                Constant(vec![0]),
                Constant(vec![4]).lift_ast().lift_ast(),
                Constant(vec![3])
            ]
            .lift_ast()
            .lift_ast()
            .lift_ast()
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
