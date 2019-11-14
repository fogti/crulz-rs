use crate::ast::{ASTNode, CmdEvalArgs, GroupType, VAN};
use delegate::delegate;
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

    #[inline]
    fn take(mut self: &mut Self) -> Self {
        std::mem::replace(&mut self, Default::default())
    }

    #[inline]
    fn simplify_inplace(&mut self) {
        *self = self.take().simplify();
    }

    /// this cleanup up the AST, opposite of two lift_ast invocations
    fn simplify(self) -> Self;

    /// this apply_arguments function applies the 'args' to the AST
    /// # Return value
    /// * `Err(idx)`: the first applied index which wasn't present in 'args'
    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize>;
}

impl MangleAST for ASTNode {
    type LiftT = VAN;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        vec![self]
    }

    fn to_str(self, escc: char) -> String {
        use ASTNode::*;
        match self {
            NullNode => String::new(),
            Constant(_, x) => x.to_string(),
            Grouped(gt, elems) => {
                let inner = elems.to_str(escc);
                let is_strict = gt == GroupType::Strict;
                let mut ret = String::with_capacity((if is_strict { 2 } else { 0 }) + inner.len());
                if is_strict {
                    ret.push('(');
                }
                ret += &inner;
                if is_strict {
                    ret.push(')');
                }
                ret
            }
            Argument { indirection, index } => {
                let mut ret = std::iter::repeat('$').take(indirection + 1).collect();
                if let Some(index) = index {
                    ret += format!("{}", index).as_str();
                }
                ret
            }
            CmdEval(cmd, args) => format!("{}({}{})", escc, cmd.to_str(escc), args.to_str(escc)),
        }
    }

    fn get_complexity(&self) -> usize {
        use ASTNode::*;
        match &self {
            NullNode => 0,
            Constant(_, x) => 1 + x.len(),
            Argument { indirection, .. } => 3 + indirection,
            Grouped(gt, x) => {
                (match *gt {
                    GroupType::Dissolving => 0,
                    GroupType::Loose => 1,
                    GroupType::Strict => 2,
                }) + x.get_complexity()
            }
            CmdEval(cmd, x) => 1 + cmd.get_complexity() + x.get_complexity(),
        }
    }

    fn simplify(mut self) -> Self {
        use ASTNode::*;
        let mut cplx = self.get_complexity();
        loop {
            match &mut self {
                Grouped(ref mut gt, ref mut x) => {
                    match x.len() {
                        0 => {
                            if *gt != GroupType::Strict {
                                self = NullNode;
                            }
                        }
                        1 => {
                            let y = x[0].take().simplify();
                            if *gt != GroupType::Strict {
                                self = y;
                            } else if let Grouped(GroupType::Dissolving, z) = y {
                                *x = z;
                            } else {
                                // swap it back, omit clone
                                x[0] = y;
                            }
                        }
                        _ => x.simplify_inplace(),
                    }
                }
                CmdEval(ref mut cmd, ref mut args) => {
                    cmd.simplify_inplace();
                    args.simplify_inplace();
                }
                _ => break,
            }
            let new_cplx = self.get_complexity();
            if new_cplx >= cplx {
                break;
            }
            cplx = new_cplx;
        }
        self
    }

    fn apply_arguments_inplace(&mut self, xargs: &CmdEvalArgs) -> Result<(), usize> {
        use ASTNode::*;
        match self {
            Argument {
                indirection: 0,
                index,
            } => {
                *self = match *index {
                    Some(index) => match xargs.0.get(index) {
                        Some(x) => x.clone(),
                        None => return Err(index),
                    },
                    None => Constant(true, crulst_atom!("$")),
                };
            }
            Argument {
                ref mut indirection,
                ..
            } => *indirection -= 1,

            Grouped(_, ref mut x) => x.apply_arguments_inplace(xargs)?,
            CmdEval(ref mut cmd, ref mut args) => {
                cmd.apply_arguments_inplace(xargs)?;
                args.apply_arguments_inplace(xargs)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl MangleAST for VAN {
    type LiftT = ASTNode;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        ASTNode::Grouped(GroupType::Dissolving, self)
    }

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
            Grouped(GroupType),
            Opaque,
        }

        self.into_iter()
            .map(|i| i.simplify())
            .group_by(|i| {
                use ASTNodeClass::*;
                match i {
                    ASTNode::Grouped(gt, x) if x.is_empty() && *gt != GroupType::Strict => NullNode,
                    ASTNode::Constant(_, x) if x.is_empty() => NullNode,
                    ASTNode::Constant(s, _) => Constant(*s),
                    ASTNode::Grouped(s, _) => Grouped(*s),
                    ASTNode::Argument { .. } | ASTNode::CmdEval(_, _) => Opaque,
                    _ => NullNode,
                }
            })
            .into_iter()
            .filter(|(d, _)| *d != ASTNodeClass::NullNode)
            .flat_map(|(d, i)| {
                use ASTNode::*;
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
                    ASTNodeClass::Grouped(GroupType::Dissolving) => i
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

    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize> {
        for i in self.iter_mut() {
            i.apply_arguments_inplace(args)?;
        }
        Ok(())
    }
}

impl MangleAST for CmdEvalArgs {
    type LiftT = ASTNode;
    #[inline]
    fn lift_ast(self) -> Self::LiftT {
        ASTNode::Grouped(GroupType::Dissolving, self.0)
    }

    fn to_str(self, escc: char) -> String {
        self.0
            .into_iter()
            .fold(String::new(), |acc, i| acc + " " + &i.to_str(escc))
    }

    fn simplify(self) -> Self {
        self.into_iter()
            .map(|i| i.simplify())
            .flat_map(|i| {
                if let ASTNode::Grouped(GroupType::Dissolving, elems) = i {
                    elems
                } else {
                    i.lift_ast()
                }
            })
            .collect()
    }

    delegate! {
        target self.0 {
            fn get_complexity(&self) -> usize;
            fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize>;
        }
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
                ASTNode::Grouped(gt, x) if gt != GroupType::Strict => x,
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
