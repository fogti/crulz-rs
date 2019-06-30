use crate::hlparser::{ASTNode, MangleAST};
use std::{borrow::Cow, collections::HashMap};

type VAN = Vec<ASTNode>;

#[derive(Clone)]
enum InterpValue {
    BuiltIn(fn(&VAN, &mut EvalContext) -> Option<VAN>),
    Data(usize, VAN),
}

type DefinesMap = HashMap<Cow<'static, str>, InterpValue>;

struct EvalContext {
    defs: DefinesMap,
}

mod builtin {
    use super::*;

    macro_rules! define_blti {
        ($name:ident,$args:ident,$ctx:ident, $body:tt) => {
            pub(super) fn $name($args: &VAN, mut $ctx: &mut EvalContext) -> Option<VAN> $body
        }
    }

    define_blti!(def, args, ctx, {
        use crate::sharpen::Classify;
        let mut unspaced = args
            .classify(|_, i| {
                if let ASTNode::Space(_) = i {
                    false
                } else {
                    true
                }
            })
            .into_iter()
            .filter(|(d, _)| *d)
            .map(|(_, i)| i)
            .flatten()
            .collect::<Vec<_>>();
        if unspaced.len() != 3 {
            return None;
        }
        let mut unpack = |x: &mut ASTNode| {
            let mut y = std::mem::replace(x, ASTNode::NullNode)
                .eval(&mut ctx)
                .lift_ast();
            y.simplify();
            y
        };
        use std::str;
        let varname = unpack(&mut unspaced[0]);
        let argc = unpack(&mut unspaced[1]);
        let varname = str::from_utf8(varname.get_constant()?)
            .expect("expected utf8 varname")
            .to_owned();
        let argc: usize = str::from_utf8(argc.get_constant()?)
            .expect("expected utf8 argc")
            .parse()
            .expect("expected number as argc");
        let value = {
            let mut y = std::mem::replace(&mut unspaced[2], ASTNode::NullNode).lift_ast();
            y.simplify();
            y
        };
        ctx.defs
            .insert(Cow::from(varname), InterpValue::Data(argc, value));
        Some(vec![])
    });
}

fn register_builtin_(
    defs: &mut DefinesMap,
    name: &'static str,
    fnx: fn(&VAN, &mut EvalContext) -> Option<VAN>,
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

fn eval_cmd(cmd: &str, args: &VAN, mut ctx: &mut EvalContext) -> Option<VAN> {
    let val = ctx.defs.get(&Cow::from(cmd))?;
    use InterpValue::*;
    match &val {
        BuiltIn(x) => x(args, &mut ctx),
        Data(0, x) => Some(x.clone()),
        Data(n, x) => {
            if args.len() < *n {
                return None;
            }
            let mut tmp = x.clone();
            for i in (0..*n).rev() {
                tmp.replace(format!("${}", i).as_bytes(), &args[i].clone().eval(ctx));
            }
            tmp.simplify();
            Some(tmp)
        }
    }
}

trait Eval {
    fn eval(self, ctx: &mut EvalContext) -> VAN;
}

impl Eval for ASTNode {
    fn eval(self, mut ctx: &mut EvalContext) -> VAN {
        if let ASTNode::CmdEval(cmd, args) = &self {
            if let Some(x) = eval_cmd(cmd, args, &mut ctx) {
                return x;
            }
        }
        vec![self]
    }
}

impl Eval for VAN {
    fn eval(self, mut ctx: &mut EvalContext) -> VAN {
        self.into_iter()
            .map(|x| x.eval(&mut ctx))
            .flatten()
            .collect()
    }
}

pub fn eval(data: VAN) -> VAN {
    let mut ctx = EvalContext::new();
    data.eval(&mut ctx)
}
