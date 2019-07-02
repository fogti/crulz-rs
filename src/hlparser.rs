use crate::llparser::Sections;
use crate::sharpen::classify_as_vec;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ASTNode {
    NullNode,

    /// Constant: is_non_space, data
    Constant(bool, Vec<u8>),

    /// Grouped: is_strict, elems
    /// loose groups are created while replacing patterns
    Grouped(bool, Box<Vec<ASTNode>>),

    CmdEval(String, Box<Vec<ASTNode>>),
}

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"
pub type VAN = Vec<ASTNode>;

pub trait MangleAST: Sized + Default + Clone {
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

    fn transform_cond<FnT>(&mut self, fnx: FnT)
    where
        FnT: FnOnce(Self) -> Option<Self>,
    {
        if let Some(x) = fnx(self.clone()) {
            *self = x;
        }
    }

    #[inline]
    fn transform_inplace<FnT>(&mut self, fnx: FnT)
    where
        FnT: FnOnce(Self) -> Self,
    {
        *self = fnx(self.take());
    }

    fn transform_recursive_cond<FnT>(&mut self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> Option<ASTNode>;

    // transform_recursive is more effective for unconditional fnx'
    // because it doesn't need to copy Self twice
    fn transform_recursive<FnT>(&mut self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> ASTNode;

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

// helper for MangleAST::simplify_inplace
#[derive(Copy, Clone, Debug, PartialEq)]
enum ASTNodeClass {
    NullNode,
    Constant(bool),
    Grouped(bool),
    CmdEval,
}

impl std::default::Default for ASTNode {
    #[inline]
    fn default() -> Self {
        ASTNode::NullNode
    }
}

impl std::default::Default for ASTNodeClass {
    #[inline]
    fn default() -> Self {
        ASTNodeClass::NullNode
    }
}

impl ASTNode {
    pub fn as_constant(&self) -> Option<&Vec<u8>> {
        match &self {
            ASTNode::Constant(_, x) => Some(x),
            _ => None,
        }
    }
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

    fn transform_recursive_cond<FnT>(mut self: &mut Self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> Option<ASTNode>,
    {
        self.transform_cond(fnx);
        use crate::hlparser::ASTNode::*;
        match &mut self {
            Grouped(_, ref mut x) | CmdEval(_, ref mut x) => {
                x.transform_recursive_cond(fnx);
            }
            _ => {}
        }
    }

    fn transform_recursive<FnT>(mut self: &mut Self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> ASTNode,
    {
        self.transform_inplace(fnx);
        use crate::hlparser::ASTNode::*;
        match &mut self {
            Grouped(_, ref mut x) | CmdEval(_, ref mut x) => {
                x.transform_recursive(fnx);
            }
            _ => {}
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
        self.into_iter().map(|i| i.to_u8v(escc)).flatten().collect()
    }
    #[inline]
    fn get_complexity(&self) -> usize {
        self.par_iter().map(|i| i.get_complexity()).sum()
    }

    fn transform_recursive_cond<FnT>(&mut self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> Option<ASTNode>,
    {
        self.par_iter_mut()
            .for_each(|i| i.transform_recursive_cond(fnx))
    }

    fn transform_recursive<FnT>(&mut self, fnx: &FnT)
    where
        FnT: Send + Sync + Fn(ASTNode) -> ASTNode,
    {
        self.par_iter_mut().for_each(|i| i.transform_recursive(fnx))
    }

    fn simplify(mut self) -> Self {
        self.par_iter_mut().for_each(|i| i.simplify_inplace());
        classify_as_vec(self, |i| {
            use crate::hlparser::ASTNodeClass::*;
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
    fn to_ast(self, escc: u8) -> VAN;
}

impl ToAST for Sections {
    fn to_ast(self, escc: u8) -> VAN {
        let mut top = Vec::<ASTNode>::new();

        for (is_cmdeval, section) in self {
            assert!(!section.is_empty());
            let slen = section.len();
            use crate::llparser::{parse_whole, IsSpace};
            if is_cmdeval {
                let first_space = section.iter().position(|&x| x.is_space());
                let rest = first_space.map(|x| &section[x + 1..]).unwrap_or(&[]);

                top.push(ASTNode::CmdEval(
                    std::str::from_utf8(&section[0..first_space.unwrap_or(slen)])
                        .expect("got non-utf8 symbol")
                        .to_owned(),
                    Box::new(crossparse!(parse_whole, rest, escc)),
                ));
            } else if section[0] == 40 && *section.last().unwrap() == 41 {
                top.push(ASTNode::Grouped(
                    true,
                    Box::new(crossparse!(parse_whole, &section[1..slen - 1], escc)),
                ));
            } else {
                top.par_extend(
                    classify_as_vec(section, |i| i.is_space())
                        .into_par_iter()
                        .map(|(ccl, x)| ASTNode::Constant(!ccl, x)),
                );
            }
        }

        top
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlparser::ASTNode::*;
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
