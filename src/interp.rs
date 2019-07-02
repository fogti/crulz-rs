use crate::hlparser::{ASTNode, MangleAST, VAN};
use std::{borrow::Cow, collections::HashMap};

#[derive(Clone)]
enum InterpValue {
    BuiltIn(Option<usize>, fn(VAN, &mut EvalContext) -> Option<ASTNode>),
    Data(usize, ASTNode),
}

type DefinesMap = HashMap<Cow<'static, str>, InterpValue>;

struct EvalContext {
    defs: DefinesMap,
}

fn args2unspaced(args: VAN) -> VAN {
    use rayon::prelude::*;
    crate::sharpen::classify_as_vec(args, |i| match i {
        ASTNode::NullNode | ASTNode::Space(_) => false,
        _ => true,
    })
    .into_par_iter()
    .filter(|(d, _)| *d)
    .map(|(_, i)| i.lift_ast().simplify())
    .collect()
}

mod builtin {
    use super::*;

    macro_rules! define_blti {
        ($name:ident($args:tt, $ctx:ident) $body:tt) => {
            #[allow(unused_parens)]
            pub(super) fn $name($args: VAN, mut $ctx: &mut EvalContext) -> Option<ASTNode> $body
        }
    }

    define_blti!(add(args, ctx) {
        let unpacked = args.into_iter().filter_map(|mut x| {
            x.eval(&mut ctx);
            x.simplify_inplace();
            Some(std::str::from_utf8(x.as_constant()?).ok()?
            .parse::<i64>()
            .expect("expected number as @param"))
        }).collect::<Vec<_>>();
        if unpacked.len() != 2 {
            // if any argument wasn't evaluated --> dropped --> different len()
            return None;
        }
        Some(ASTNode::Constant((unpacked[0] + unpacked[1]).to_string().into_bytes()))
    });

    define_blti!(def((mut args), ctx) {
        if args.len() < 3 {
            return None;
        }
        let mut unpack = |x: &mut ASTNode| {
            let mut y = x.take();
            y.eval(&mut ctx);
            y.simplify()
        };
        use std::str;
        let varname = unpack(&mut args[0]);
        let argc = unpack(&mut args[1]);
        let varname = str::from_utf8(varname.as_constant()?)
            .expect("expected utf8 varname")
            .to_owned();
        let argc: usize = str::from_utf8(argc.as_constant()?)
            .expect("expected utf8 argc")
            .parse()
            .expect("expected number as argc");
        let value = args[2..].to_vec().lift_ast().simplify();
        ctx.defs
            .insert(Cow::from(varname), InterpValue::Data(argc, value));
        Some(ASTNode::NullNode)
    });

    define_blti!(une(args, ctx) {
        Some(args.into_iter().map(|mut x| {
            x.eval(&mut ctx);
            x.simplify_inplace();
            if let ASTNode::Grouped(ref mut is_strict, _) = x {
                *is_strict = false;
            }
            x
        }).collect::<Vec<_>>().lift_ast().simplify())
    });
}

fn register_builtin_(
    defs: &mut DefinesMap,
    name: &'static str,
    argc: Option<usize>,
    fnx: fn(VAN, &mut EvalContext) -> Option<ASTNode>,
) {
    defs.insert(Cow::from(name), InterpValue::BuiltIn(argc, fnx));
}

impl EvalContext {
    fn new() -> Self {
        let mut defs = DefinesMap::new();
        macro_rules! register_builtins {
            ($defs:ident, $($fn:ident $ac:expr),+) => {
                $(
                register_builtin_(&mut $defs, stringify!($fn), $ac, builtin::$fn);
                )+
            };
        }
        register_builtins!(defs, add Some(2), def None, une None);
        Self { defs }
    }
}

fn eval_cmd(cmd: &str, mut args: VAN, mut ctx: &mut EvalContext) -> Option<ASTNode> {
    let val = ctx.defs.get(&Cow::from(cmd))?;
    use crate::interp::InterpValue::*;
    match &val {
        BuiltIn(a, x) => match a {
            Some(n) if args.len() != *n => None,
            _ => x(args, &mut ctx),
        },
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
    fn eval(mut self: &mut Self, mut ctx: &mut EvalContext) {
        use crate::hlparser::ASTNode::*;
        match &mut self {
            CmdEval(cmd, args) => {
                if let Some(x) = eval_cmd(cmd, args2unspaced(*args.clone()), &mut ctx) {
                    *self = x;
                }
            }
            Grouped(_, x) => {
                x.eval(&mut ctx);
            }
            _ => {}
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
    let mut cplx = data.get_complexity();
    loop {
        data.eval(&mut ctx);
        data.simplify_inplace();
        let new_cplx = data.get_complexity();
        if new_cplx == cplx {
            break;
        }
        cplx = new_cplx;
    }
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
