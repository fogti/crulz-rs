use crate::hlparser::{ASTNode, MangleAST};
use std::{borrow::Cow, collections::HashMap};

type VAN = Vec<ASTNode>;

#[derive(Clone)]
enum InterpValue {
    BuiltIn(fn(VAN, &mut EvalContext) -> Option<ASTNode>),
    Data(usize, ASTNode),
}

type DefinesMap = HashMap<Cow<'static, str>, InterpValue>;

struct EvalContext {
    defs: DefinesMap,
}

fn args2unspaced(args: VAN) -> VAN {
    use crate::sharpen::ClassifyIter;
    args.into_iter()
        .classify_iter(|i| match i {
            ASTNode::NullNode | ASTNode::Space(_) => false,
            _ => true,
        })
        .filter(|(d, _)| *d)
        .map(|(_, i)| i.lift_ast().simplify())
        .collect()
}

mod builtin {
    use super::*;

    macro_rules! define_blti {
        ($name:ident,$args:ident,$ctx:ident, $body:tt) => {
            pub(super) fn $name(mut $args: VAN, mut $ctx: &mut EvalContext) -> Option<ASTNode> $body
        }
    }

    define_blti!(def, args, ctx, {
        if args.len() != 3 {
            return None;
        }
        let mut unpack = |x: &mut ASTNode| {
            let mut y = std::mem::replace(x, ASTNode::NullNode);
            y.eval(&mut ctx);
            y.simplify()
        };
        use std::str;
        let varname = unpack(&mut args[0]);
        let argc = unpack(&mut args[1]);
        let varname = str::from_utf8(varname.get_constant()?)
            .expect("expected utf8 varname")
            .to_owned();
        let argc: usize = str::from_utf8(argc.get_constant()?)
            .expect("expected utf8 argc")
            .parse()
            .expect("expected number as argc");
        let value = std::mem::replace(&mut args[2], ASTNode::NullNode).simplify();
        ctx.defs
            .insert(Cow::from(varname), InterpValue::Data(argc, value));
        Some(ASTNode::NullNode)
    });
}

fn register_builtin_(
    defs: &mut DefinesMap,
    name: &'static str,
    fnx: fn(VAN, &mut EvalContext) -> Option<ASTNode>,
) {
    defs.insert(Cow::from(name), InterpValue::BuiltIn(fnx));
}

macro_rules! register_builtin {
    ($defs:ident, $name:expr, $fn:ident) => {
        register_builtin_(&mut $defs, $name, builtin::$fn);
    };
}

impl EvalContext {
    fn new() -> Self {
        let mut defs = DefinesMap::new();
        register_builtin!(defs, "def", def);
        Self { defs }
    }
}

fn eval_cmd(cmd: &str, mut args: VAN, mut ctx: &mut EvalContext) -> Option<ASTNode> {
    let val = ctx.defs.get(&Cow::from(cmd))?;
    use crate::interp::InterpValue::*;
    match &val {
        BuiltIn(x) => x(args, &mut ctx),
        Data(0, x) => Some(x.clone()),
        Data(n, x) => {
            if args.len() < *n {
                return None;
            }
            let mut tmp = x.clone();
            for i in (0..*n).rev() {
                args[i].eval(ctx);
                tmp = tmp
                    .replace(format!("${}", i).as_bytes(), &args[i])
                    .simplify();
            }
            Some(tmp)
        }
    }
}

trait Eval {
    fn eval(&mut self, ctx: &mut EvalContext);
}

impl Eval for ASTNode {
    fn eval(&mut self, mut ctx: &mut EvalContext) {
        if let ASTNode::CmdEval(cmd, args) = &self {
            if let Some(x) = eval_cmd(cmd, args2unspaced(*args.clone()), &mut ctx) {
                *self = x;
            }
        }
    }
}

impl Eval for VAN {
    fn eval(&mut self, mut ctx: &mut EvalContext) {
        for i in self {
            i.eval(&mut ctx);
        }
    }
}

pub fn eval(data: &mut VAN) {
    let mut ctx = EvalContext::new();
    data.eval(&mut ctx);
    data.simplify_inplace();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args2unspaced() {
        use ASTNode::*;
        assert_eq!(
            args2unspaced(vec![
                Constant(vec![0]),
                Space(vec![0]),
                Constant(vec![0]),
                Constant(vec![0]),
                Space(vec![0])
            ]),
            vec![Constant(vec![0]), Constant(vec![0, 0])]
        );
    }
}
