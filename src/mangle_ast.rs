use crate::hlparser::{ASTNode, VAN};
use crate::sharpen::classify_as_vec;
use rayon::prelude::*;

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

// helper for MangleAST::simplify_inplace
#[derive(Copy, Clone, Debug, PartialEq)]
enum ASTNodeClass {
    NullNode,
    Constant(bool),
    Grouped(bool),
    CmdEval,
}

impl std::default::Default for ASTNodeClass {
    #[inline]
    fn default() -> Self {
        ASTNodeClass::NullNode
    }
}

pub trait MangleAST: Default {
    type LiftT;
    // lift the AST one level up (ASTNode -> VAN || VAN -> ASTNode),
    // used as helper for MangleAST::simplify_inplace and others
    // to convert to the appropriate datatype
    fn lift_ast(self) -> Self::LiftT;

    fn to_u8v(self, escc: u8) -> Vec<u8>;

    /// helper for MangleAST::simplify_inplace
    fn get_complexity(&self) -> usize;

    fn take(mut self: &mut Self) -> Self {
        std::mem::replace(&mut self, Default::default())
    }

    #[inline]
    fn transform_inplace<FnT>(&mut self, fnx: FnT)
    where
        FnT: FnOnce(Self) -> Self,
    {
        *self = fnx(self.take());
    }

    #[inline]
    fn simplify_inplace(&mut self) {
        self.transform_inplace(|x| x.simplify());
    }

    /// this replace function works on byte-basis and honours ASTNode boundaries
    fn replace_inplace(&mut self, from: &[u8], to: &ASTNode);
    fn replace(self, from: &[u8], to: &ASTNode) -> Self;

    /// this cleanup up the AST, opposite of two lift_ast invocations
    fn simplify(self) -> Self;
}

impl MangleAST for ASTNode {
    type LiftT = VAN;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }

    fn to_u8v(self, escc: u8) -> Vec<u8> {
        use crate::hlparser::ASTNode::*;
        match self {
            NullNode => vec![],
            Constant(_, x) => x,
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
        use crate::hlparser::ASTNode::*;
        match &self {
            NullNode => 0,
            Constant(_, x) => 1 + x.len(),
            Grouped(_, x) => 2 + x.get_complexity(),
            CmdEval(cmd, x) => 1 + cmd.len() + x.get_complexity(),
        }
    }

    fn simplify(mut self) -> Self {
        use crate::hlparser::ASTNode::*;
        let mut cplx = self.get_complexity();
        while let Grouped(is_strict, ref mut x) = &mut self {
            match x.len() {
                0 => {
                    if !*is_strict {
                        self = NullNode;
                    }
                }
                1 => {
                    let y = x[0].take();
                    if *is_strict {
                        if let Grouped(_, z) = y {
                            *x = z;
                        } else {
                            // swap it back, omit clone
                            x[0] = y.simplify();
                        }
                    } else {
                        self = y;
                    }
                }
                _ => x.simplify_inplace(),
            }
            let new_cplx = self.get_complexity();
            if new_cplx >= cplx {
                break;
            }
            cplx = new_cplx;
        }
        self
    }

    #[inline]
    fn replace_inplace(&mut self, from: &[u8], to: &ASTNode) {
        self.transform_inplace(|x| x.replace(from, to))
    }

    fn replace(self, from: &[u8], to: &ASTNode) -> Self {
        use crate::hlparser::ASTNode::*;
        match self {
            Constant(true, x) => {
                let flen = from.len();
                let mut skp: usize = 0;
                classify_as_vec(x, |&i| {
                    let ret = skp != flen && i == from[skp];
                    skp = if ret { skp + 1 } else { 0 };
                    ret
                })
                .into_par_iter()
                .map(|(d, i)| {
                    if d && i.len() == flen {
                        to.clone()
                    } else {
                        Constant(true, i)
                    }
                })
                .collect::<Vec<_>>()
                .lift_ast()
            }
            Grouped(is_strict, x) => Grouped(is_strict, Box::new(x.replace(from, to))),
            CmdEval(mut cmd, args) => {
                // mangle cmd
                if let Constant(true, to2) = &to {
                    use std::str;
                    if let Ok(from2) = str::from_utf8(from) {
                        if let Ok(to3) = str::from_utf8(&to2) {
                            cmd = cmd.replace(from2, to3);
                        }
                    }
                }

                // mangle args
                CmdEval(cmd, Box::new(args.replace(from, to)))
            }
            // we ignore spaces
            _ => self,
        }
    }
}

impl MangleAST for VAN {
    type LiftT = ASTNode;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        ASTNode::Grouped(false, Box::new(self))
    }

    #[inline]
    fn to_u8v(self, escc: u8) -> Vec<u8> {
        self.into_par_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
    #[inline]
    fn get_complexity(&self) -> usize {
        self.par_iter().map(|i| i.get_complexity()).sum()
    }

    fn simplify(mut self) -> Self {
        self.par_iter_mut().for_each(|i| i.simplify_inplace());
        classify_as_vec(self, |i| {
            use crate::mangle_ast::ASTNodeClass::*;
            match &i {
                ASTNode::Grouped(false, x) if x.is_empty() => NullNode,
                ASTNode::Constant(_, x) if x.is_empty() => NullNode,
                ASTNode::Constant(s, _) => Constant(*s),
                ASTNode::Grouped(s, _) => Grouped(*s),
                ASTNode::CmdEval(_, _) => CmdEval,
                _ => NullNode,
            }
        })
        .into_par_iter()
        .map(|(d, i)| {
            use crate::hlparser::ASTNode::*;
            macro_rules! recollect {
                ($i:expr, $in:pat, $out:expr) => {
                    $i.into_par_iter()
                        .map(|j| {
                            if let $in = j {
                                $out
                            } else {
                                unsafe { std::hint::unreachable_unchecked() }
                            }
                        })
                        .flatten()
                        .collect()
                };
            };
            match d {
                ASTNodeClass::NullNode => NullNode.lift_ast(),
                _ if i.len() < 2 => i,
                ASTNodeClass::Constant(x) => {
                    Constant(x, recollect!(i, Constant(_, y), y)).lift_ast()
                }
                ASTNodeClass::Grouped(false) => recollect!(i, Grouped(_, x), *x),
                _ => i,
            }
        })
        .flatten()
        .filter(|i| {
            if let ASTNode::NullNode = i {
                false
            } else {
                true
            }
        })
        .collect::<Self>()
    }
    #[inline]
    fn replace_inplace(&mut self, from: &[u8], to: &ASTNode) {
        self.par_iter_mut()
            .for_each(|i| i.replace_inplace(from, to));
    }
    #[inline]
    fn replace(mut self, from: &[u8], to: &ASTNode) -> Self {
        self.replace_inplace(from, to);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::ASTNode::*;
    extern crate test;

    #[test]
    fn test_replace() {
        assert_eq!(
            vec![Constant(true, vec![0, 1, 2, 3])]
                .lift_ast()
                .replace(&vec![1, 2], &Constant(true, vec![4])),
            vec![
                Constant(true, vec![0]),
                Constant(true, vec![4]),
                Constant(true, vec![3])
            ]
            .lift_ast()
            .lift_ast()
            .lift_ast()
        );
    }

    #[test]
    fn test_simplify() {
        let ast = vec![
            Constant(true, vec![0]),
            Constant(true, vec![4])
                .lift_ast()
                .lift_ast()
                .lift_ast()
                .lift_ast(),
            Constant(true, vec![3]),
        ]
        .lift_ast()
        .lift_ast()
        .lift_ast();
        assert_eq!(ast.simplify(), Constant(true, vec![0, 4, 3]));
    }

    #[bench]
    fn bench_replace(b: &mut test::Bencher) {
        let ast = Constant(true, vec![0, 1, 2, 3]).lift_ast().lift_ast();
        b.iter(|| ast.clone().replace(&vec![1, 2], &Constant(true, vec![4])));
    }

    #[bench]
    fn bench_simplify(b: &mut test::Bencher) {
        let ast = vec![
            Constant(true, vec![0]),
            Constant(true, vec![4])
                .lift_ast()
                .lift_ast()
                .lift_ast()
                .lift_ast(),
            Constant(true, vec![3]),
        ]
        .lift_ast()
        .lift_ast()
        .lift_ast();
        b.iter(|| ast.clone().simplify());
    }
}
