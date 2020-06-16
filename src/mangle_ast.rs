use crate::ast::{ASTNode, CmdEvalArgs, GroupType, LiftAST, VAN};
use delegate_attr::delegate;
use itertools::Itertools;

// do NOT "use ASTNode::*;" here, because sometimes we want to "use ASTNodeClass::*;"

pub trait MangleAST: Default {
    fn to_vec(self, escc: u8) -> Vec<u8>;

    /// helper for MangleAST::simplify and interp::eval
    fn get_complexity(&self) -> usize;

    #[inline(always)]
    fn take(&mut self) -> Self {
        std::mem::take(self)
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
    fn to_vec(self, escc: u8) -> Vec<u8> {
        use ASTNode::*;
        match self {
            NullNode => Vec::new(),
            Constant(_, x) => x.into(),
            Grouped(gt, elems) => {
                let inner = elems.to_vec(escc);
                if gt == GroupType::Strict {
                    [b"(", &inner[..], b")"]
                        .iter()
                        .flat_map(|i| *i)
                        .copied()
                        .collect()
                } else {
                    inner
                }
            }
            Argument { indirection, index } => std::iter::repeat(b'$')
                .take(indirection + 1)
                .chain(
                    index
                        .as_ref()
                        .map(usize::to_string)
                        .iter()
                        .flat_map(|i| i.bytes()),
                )
                .collect(),
            CmdEval(cmd, args) => {
                let mut ret = Vec::new();
                ret.push(escc);
                ret.push(b'(');
                ret.extend_from_slice(&cmd.to_vec(escc)[..]);
                ret.extend_from_slice(&args.to_vec(escc)[..]);
                ret.push(b')');
                ret
            }
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
                                break;
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
                    None => Constant(true, vec![b'$'].into()),
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
    fn to_vec(self, escc: u8) -> Vec<u8> {
        self.into_iter()
            .map(|i| i.to_vec(escc))
            .intersperse(vec![b' '])
            .flatten()
            .collect()
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
                        i.flat_map(|j| {
                            if let Constant(_, y) = j {
                                Vec::from(y)
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                    )
                    .lift_ast(),
                    ASTNodeClass::Grouped(GroupType::Dissolving) => i
                        .flat_map(|j| {
                            if let Grouped(_, x) = j {
                                x
                            } else {
                                unreachable!()
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
    fn to_vec(self, escc: u8) -> Vec<u8> {
        if self.0.is_empty() {
            Vec::new()
        } else {
            let mut ret = vec![b' '];
            ret.extend_from_slice(&self.0.to_vec(escc)[..]);
            ret
        }
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

    #[delegate(self.0)]
    fn get_complexity(&self) -> usize {}

    #[delegate(self.0)]
    fn apply_arguments_inplace(&mut self, args: &CmdEvalArgs) -> Result<(), usize> {}
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
            .flat_map(|i| match i {
                ASTNode::NullNode => vec![],
                ASTNode::Grouped(gt, x) if gt != GroupType::Strict => x.compact_toplevel(),
                _ => vec![i],
            })
            // 2. aggressive concat constant-after-constants
            .peekable()
            .batching(|it| {
                Some(match it.next()? {
                    ASTNode::Constant(mut risp, mut rdat) => {
                        while let Some(ASTNode::Constant(isp, ref dat)) = it.peek() {
                            risp |= isp;
                            rdat.extend_from_slice(&dat[..]);
                            it.next();
                        }
                        ASTNode::Constant(risp, rdat.into())
                    }
                    x => x,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ASTNode::*;
    use super::*;

    #[test]
    fn test_simplify() {
        let ast = vec![
            Constant(true, b"a".to_vec()),
            Constant(true, b"b".to_vec())
                .lift_ast()
                .lift_ast()
                .lift_ast()
                .lift_ast(),
            Constant(true, b"c".to_vec()),
        ]
        .lift_ast()
        .lift_ast()
        .lift_ast();
        assert_eq!(ast.simplify(), Constant(true, b"abc".to_vec()));
    }

    #[test]
    fn test_compact_tl() {
        let ast = vec![
            Constant(true, b"a".to_vec()),
            Constant(false, b"b".to_vec()),
            Constant(true, b"c".to_vec()),
        ]
        .compact_toplevel();
        assert_eq!(ast, vec![Constant(true, b"abc".to_vec())]);
    }
}
