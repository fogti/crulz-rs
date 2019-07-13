use crate::ast::{ASTNode, VAN};
use itertools::Itertools;

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

pub trait MangleAST: Default {
    type LiftT;
    // lift the AST one level up (ASTNode -> VAN || VAN -> ASTNode),
    // used as helper for MangleAST::simplify_inplace and others
    // to convert to the appropriate datatype
    fn lift_ast(self) -> Self::LiftT;

    fn to_str(self, escc: char) -> String;

    /// helper for MangleAST::simplify and interp::eval
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
    fn replace_inplace(&mut self, from: &str, to: &ASTNode);
    fn replace(self, from: &str, to: &ASTNode) -> Self;

    /// this cleanup up the AST, opposite of two lift_ast invocations
    fn simplify(self) -> Self;
}

impl MangleAST for ASTNode {
    type LiftT = VAN;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }

    fn to_str(self, escc: char) -> String {
        use crate::ast::ASTNode::*;
        match self {
            NullNode => String::new(),
            Constant(_, x) => x.to_string(),
            Grouped(is_strict, elems) => {
                let inner = elems.to_str(escc);
                let mut ret = String::with_capacity(2 + inner.len());
                if is_strict {
                    ret.push('(');
                }
                ret += &inner;
                if is_strict {
                    ret.push(')');
                }
                ret
            }
            CmdEval(cmd, args) => {
                let cmd = cmd.to_str(escc);
                let args = args.to_str(escc);
                let mut ret = String::with_capacity(4 + cmd.len() + args.len());
                ret.push(escc);
                ret.push('(');
                ret += &cmd;
                if !args.is_empty() {
                    ret.push(' ');
                    ret += &args;
                }
                ret.push(')');
                ret
            }
        }
    }

    fn get_complexity(&self) -> usize {
        use crate::ast::ASTNode::*;
        match &self {
            NullNode => 0,
            Constant(_, x) => 1 + x.len(),
            Grouped(_, x) => 2 + x.get_complexity(),
            CmdEval(cmd, x) => 1 + cmd.get_complexity() + x.get_complexity(),
        }
    }

    fn simplify(mut self) -> Self {
        use crate::ast::ASTNode::*;
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
    fn replace_inplace(&mut self, from: &str, to: &ASTNode) {
        self.transform_inplace(|x| x.replace(from, to))
    }

    fn replace(self, from: &str, to: &ASTNode) -> Self {
        use crate::ast::ASTNode::*;
        match self {
            Constant(true, x) => x
                .split(from)
                .map(|i| Constant(true, i.into()))
                .intersperse(to.clone())
                .collect::<Vec<_>>()
                .lift_ast(),
            Grouped(is_strict, x) => Grouped(is_strict, x.replace(from, to)),
            CmdEval(cmd, args) => CmdEval(cmd.replace(from, to), args.replace(from, to)),
            // we ignore spaces
            _ => self,
        }
    }
}

impl MangleAST for VAN {
    type LiftT = ASTNode;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        ASTNode::Grouped(false, self)
    }

    #[inline]
    fn to_str(self, escc: char) -> String {
        self.into_iter()
            .fold(String::new(), |acc, i| acc + &i.to_str(escc))
    }
    #[inline]
    fn get_complexity(&self) -> usize {
        self.iter().map(|i| i.get_complexity()).sum()
    }

    fn simplify(self) -> Self {
        #[derive(PartialEq)]
        enum ASTNodeClass {
            NullNode,
            Constant(bool),
            Grouped(bool),
            CmdEval,
        }

        self.into_iter()
            .map(|i| i.simplify())
            .group_by(|i| {
                use ASTNodeClass::*;
                match i {
                    ASTNode::Grouped(false, x) if x.is_empty() => NullNode,
                    ASTNode::Constant(_, x) if x.is_empty() => NullNode,
                    ASTNode::Constant(s, _) => Constant(*s),
                    ASTNode::Grouped(s, _) => Grouped(*s),
                    ASTNode::CmdEval(_, _) => CmdEval,
                    _ => NullNode,
                }
            })
            .into_iter()
            .filter(|(d, _)| *d != ASTNodeClass::NullNode)
            .flat_map(|(d, i)| {
                use crate::ast::ASTNode::*;
                match d {
                    ASTNodeClass::Constant(x) => Constant(
                        x,
                        i.map(|j| {
                            if let Constant(_, y) = j {
                                y
                            } else {
                                unsafe { std::hint::unreachable_unchecked() }
                            }
                        })
                        .fold(String::new(), |acc, i| acc + &i)
                        .into(),
                    )
                    .lift_ast(),
                    ASTNodeClass::Grouped(false) => i
                        .flat_map(|j| {
                            if let Grouped(_, x) = j {
                                x
                            } else {
                                unsafe { std::hint::unreachable_unchecked() }
                            }
                        })
                        .collect(),
                    _ => i.collect(),
                }
            })
            .collect()
    }
    #[inline]
    fn replace_inplace(&mut self, from: &str, to: &ASTNode) {
        self.iter_mut().for_each(|i| i.replace_inplace(from, to));
    }
    #[inline]
    fn replace(mut self, from: &str, to: &ASTNode) -> Self {
        self.replace_inplace(from, to);
        self
    }
}

pub trait MangleASTExt: MangleAST {
    fn compact_toplevel(self) -> Self;
}

impl MangleASTExt for VAN {
    fn compact_toplevel(self) -> Self {
        // we are at the top level, wo can inline non-strict groups
        // and then put all constants heaps into single constants
        self.into_iter()
            // 1. inline non-strict groups
            .map(|i| match i {
                ASTNode::NullNode => vec![],
                ASTNode::Grouped(false, x) => x,
                _ => vec![i],
            })
            .flatten()
            // 2. aggressive concat constant-after-constants
            .peekable()
            .batching(|it| {
                let (mut risp, mut rdat) = match it.next()? {
                    ASTNode::Constant(isp, dat) => (isp, dat.to_string()),
                    x => return Some(x),
                };
                while let Some(ASTNode::Constant(isp, ref dat)) = it.peek() {
                    risp |= isp;
                    rdat += &dat;
                    it.next();
                }
                Some(ASTNode::Constant(risp, rdat.into()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ASTNode::*;
    use super::*;
    extern crate test;

    #[test]
    fn test_replace() {
        assert_eq!(
            vec![Constant(true, "abcd".into())]
                .lift_ast()
                .replace("bc", &Constant(true, "e".into())),
            vec![
                Constant(true, "a".into()),
                Constant(true, "e".into()),
                Constant(true, "d".into())
            ]
            .lift_ast()
            .lift_ast()
            .lift_ast()
        );
    }

    #[test]
    fn test_simplify() {
        let ast = vec![
            Constant(true, "a".into()),
            Constant(true, "b".into())
                .lift_ast()
                .lift_ast()
                .lift_ast()
                .lift_ast(),
            Constant(true, "c".into()),
        ]
        .lift_ast()
        .lift_ast()
        .lift_ast();
        assert_eq!(ast.simplify(), Constant(true, "abc".into()));
    }

    #[test]
    fn test_compact_tl() {
        let ast = vec![
            Constant(true, "a".into()),
            Constant(false, "b".into()),
            Constant(true, "c".into()),
        ]
        .compact_toplevel();
        assert_eq!(ast, vec![Constant(true, "abc".into())]);
    }

    #[bench]
    fn bench_replace(b: &mut test::Bencher) {
        let ast = Constant(true, "abcd".into()).lift_ast().lift_ast();
        b.iter(|| ast.clone().replace("bc", &Constant(true, "d".into())));
    }

    #[bench]
    fn bench_simplify(b: &mut test::Bencher) {
        let ast = vec![
            Constant(true, "a".into()),
            Constant(true, "b".into())
                .lift_ast()
                .lift_ast()
                .lift_ast()
                .lift_ast(),
            Constant(true, "c".into()),
        ]
        .lift_ast()
        .lift_ast()
        .lift_ast();
        b.iter(|| ast.clone().simplify());
    }

    #[bench]
    fn bench_compact_tl(b: &mut test::Bencher) {
        let ast = vec![
            Constant(true, "a".into()),
            Constant(false, "b".into()).lift_ast().lift_ast(),
            Constant(true, "a".into()),
            Constant(false, "b".into()).lift_ast().lift_ast(),
            Constant(true, "c".into()),
        ]
        .simplify();
        b.iter(|| ast.clone().compact_toplevel());
    }
}
