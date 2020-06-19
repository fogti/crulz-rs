use crate::{
    ast::{CmdEvalArgs, GroupType, Lift as _, Mangle, MangleExt as _, Node as ASTNode, VAN},
    parser::Options as ParserOptions,
};
#[cfg(feature = "compile")]
use anyhow::Context;
use std::{collections::HashMap, marker::PhantomData, path::Path};
use {atoi::atoi, cfg_if::cfg_if, lazy_static::lazy_static};

#[derive(Clone, Copy)]
pub enum BuiltInFn {
    /// manual built-in functions decide for themselves which arguments get evaluated
    /// and are called with a reference to the evaluation context
    Manual(fn(&mut CmdEvalArgs, &mut EvalContext) -> Option<ASTNode>),

    /// automatic built-in functions are called with partially evaluated arguments and
    /// without a reference to the evaluation context
    Automatic(fn(&[ASTNode]) -> Option<ASTNode>),
}

type DefinesMap = HashMap<Vec<u8>, (usize, ASTNode)>;
type ProcDefinesMap = HashMap<Vec<u8>, (Option<usize>, BuiltInFn)>;
type CompilatesMap<'a> = HashMap<&'a Path, &'a Path>;

pub const SUPPORTS_COMPILATION: bool = std::cfg!(feature = "compile");

pub struct EvalContext<'a> {
    pub defs: DefinesMap,
    pub procdefs: ProcDefinesMap,
    pub opts: ParserOptions,
    #[cfg_attr(not(feature = "compile"), allow(unused))]
    pub comp_map: CompilatesMap<'a>,

    _non_exhaustive: PhantomData<()>,
}

#[cfg(feature = "compile")]
impl EvalContext<'_> {
    fn load_from_compfile<P>(&mut self, compf: &P) -> Result<VAN, anyhow::Error>
    where
        P: AsRef<Path> + ?Sized,
    {
        let compf = compf.as_ref();
        let fh = readfilez::read_from_file(std::fs::File::open(compf))
            .with_context(|| format!("Unable to open compfile '{}'", compf.display()))?;
        let mut z = flate2::read::DeflateDecoder::new(fh.as_slice());
        let content: VAN = bincode::deserialize_from(&mut z)
            .with_context(|| format!("Unable to read compfile '{}'", compf.display()))?;
        let ins_defs: DefinesMap = bincode::deserialize_from(&mut z)
            .with_context(|| format!("Unable to read compfile '{}'", compf.display()))?;
        self.defs.extend(ins_defs.into_iter());
        Ok(content)
    }

    #[cold]
    fn save_to_compfile<P>(&self, compf: &P, content: &VAN) -> Result<(), anyhow::Error>
    where
        P: AsRef<Path> + ?Sized,
    {
        let compf = compf.as_ref();
        let fh = std::fs::File::create(compf)
            .with_context(|| format!("Failed to create compfile '{}'", compf.display()))?;
        let mut z = flate2::write::DeflateEncoder::new(fh, flate2::Compression::default());
        bincode::serialize_into(&mut z, content)
            .with_context(|| format!("Failed to write compfile '{}'", compf.display()))?;
        bincode::serialize_into(&mut z, &self.defs)
            .with_context(|| format!("Failed to write compfile '{}'", compf.display()))?;
        Ok(())
    }
}

fn unpack(x: &mut ASTNode, ctx: &mut EvalContext<'_>) -> Option<Vec<u8>> {
    x.eval(ctx);
    x.conv_to_constant().map(|y| y.into_owned())
}

fn uneg(mut arg: ASTNode) -> ASTNode {
    if let ASTNode::Grouped { ref mut typ, .. } = arg {
        *typ = GroupType::Dissolving;
    }
    arg
}

fn fe_elems(x: &ASTNode) -> Option<VAN> {
    match x {
        ASTNode::Grouped { ref elems, .. } => Some(elems.clone()),
        _ => None,
    }
}

macro_rules! define_blti {
    (($args:pat | $ac:expr, $ctx:pat) $body:ident) => {{
        /* fn blti($args: &mut CmdEvalArgs, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body */
        (Some($ac), BuiltInFn::Manual($body))
    }};
    (($args:pat | $ac:expr) $body:ident) => {{
        /* fn blti($args: VAN) -> Option<ASTNode> $body */
        (Some($ac), BuiltInFn::Automatic($body))
    }};
    (($args:pat, $ctx:pat) $body:ident) => {{
        /* fn blti($args: &mut CmdEvalArgs, $ctx: &mut EvalContext<'_>) -> Option<ASTNode> $body */
        (None, BuiltInFn::Manual($body))
    }};
    (($args:pat) $body:ident) => {{
        /* fn blti($args: VAN) -> Option<ASTNode> $body */
        (None, BuiltInFn::Automatic($body))
    }};
}

macro_rules! define_bltins {
    ($($name:expr => $a2:tt $body:tt,)*) => {
        maplit::hashmap! {
            $(($name.to_vec()) => define_blti!($a2 $body),)*
        }
    }
}

lazy_static! {
    static ref BUILTINS: ProcDefinesMap = {
        define_bltins! {
            b"add"           => (args | 2     ) blti_add,
            b"curry"         => (args    , ctx) blti_curry,
            b"def"           => (args    , ctx) blti_def,
            b"def-lazy"      => (args    , ctx) blti_def_lazy,
            b"foreach"       => (args | 2, ctx) blti_foreach,
            b"fseq"          => (args    , ctx) blti_fseq,
            b"include"       => (args | 1, ctx) blti_include,
            b"lambda"        => (args         ) blti_lambda,
            b"lambda-lazy"   => (args    , ctx) blti_lambda_lazy,
            b"lambda-strict" => (args    , ctx) blti_lambda_strict,
            b"pass"          => (args         ) blti_pass,
            b"suppress"      => (_args        ) blti_suppress,
            b"undef"         => (args | 1, ctx) blti_undef,
            b"une"           => (args         ) blti_une,
            b"unee"          => (args         ) blti_unee,
        }
    };
}

fn blti_add(args: &[ASTNode]) -> Option<ASTNode> {
    let unpacked = args
        .iter()
        .filter_map(|x| Some(atoi::<i64>(x.as_constant()?).expect("expected number as @param")))
        .collect::<Vec<_>>();
    if unpacked.len() != 2 {
        None
    } else {
        Some(ASTNode::Constant {
            non_space: true,
            data: (unpacked[0] + unpacked[1]).to_string().into(),
        })
    }
}

fn blti_curry(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    match args.len() {
        0 => Some(ASTNode::NullNode),
        1 => Some(args.0[0].clone()),
        _ if !args.eval(ctx) => None,
        _ => {
            let mut args = args.clone();
            let mut ret = args.0.remove(0);
            if let ASTNode::Constant { ref data, .. } = &ret {
                let cmd: &[u8] = &*data;
                let (argc, body) = if let Some(a) = ctx.procdefs.get(cmd) {
                    // LIMITATION: we can't curry proc-fn's with variable argc
                    let a = a.0?;
                    (
                        a,
                        ASTNode::CmdEval {
                            cmd: vec![ret],
                            args: (0..a)
                                .map(|i| ASTNode::Argument {
                                    indirection: 0,
                                    index: Some(i),
                                })
                                .collect(),
                        },
                    )
                } else {
                    ctx.defs.get(cmd)?.clone()
                };
                ret = ASTNode::Lambda {
                    argc,
                    body: Box::new(body),
                };
            }
            ret.curry_inplace(&args);
            Some(ret)
        }
    }
}

fn blti_def(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let args = &mut args.0;
    if args.len() < 2 || !args.iter_mut().all(|i| i.eval(ctx)) {
        return None;
    }
    let varname = args[0].conv_to_constant()?.into_owned();
    let (argc, body) = if args.len() > 2 {
        (
            atoi(&args[1].conv_to_constant()?).expect("expected number as argc"),
            args[2..].to_vec().lift_ast(),
        )
    } else if let ASTNode::Lambda { argc, ref body } = &args[1] {
        (*argc, *(*body).clone())
    } else {
        (0, args[1].clone())
    };
    ctx.defs.insert(varname, (argc, body.simplify()));
    Some(ASTNode::NullNode)
}

fn blti_def_lazy(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let args = &mut args.0;
    if args.len() < 2 {
        return None;
    }
    let varname = unpack(&mut args[0], ctx)?;
    let definition = if args.len() == 2 {
        match &args[1] {
            ASTNode::Lambda { argc, ref body } => (*argc, (*body).clone().simplify()),
            x @ ASTNode::Constant { .. } => (0, x.clone().simplify()),
            _ => return None,
        }
    } else {
        let argc: usize = atoi(&unpack(&mut args[1], ctx)?).expect("expected number as argc");
        (argc, args[2..].to_vec().lift_ast().simplify())
    };
    ctx.defs.insert(varname, definition);
    Some(ASTNode::NullNode)
}

fn blti_foreach(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let args = &mut args.0;
    args[0].eval(ctx);
    let mut elems = CmdEvalArgs::from_wsdelim(fe_elems(&args[0])?)
        .into_iter()
        .map(|i| {
            if let ASTNode::Grouped { elems, .. } = i {
                CmdEvalArgs::from_wsdelim(elems)
            } else {
                CmdEvalArgs(i.lift_ast())
            }
        });

    Some(
        match &args[1] {
            ASTNode::Constant {
                non_space: false, ..
            } => unreachable!(),
            ASTNode::Constant { .. } | ASTNode::Lambda { .. } => {
                // construct a function call
                let mut tmp_cmd = vec![args[1].clone()];
                elems.fold(Vec::new(), |mut acc, mut tmp_args| {
                    acc.push(
                        if let Some(x) = eval_cmd(&mut tmp_cmd, &mut tmp_args, ctx) {
                            x
                        } else {
                            ASTNode::CmdEval {
                                cmd: tmp_cmd.clone(),
                                args: tmp_args,
                            }
                        },
                    );
                    acc
                })
            }
            _ => elems.try_fold(Vec::new(), |mut acc, i| {
                let mut cur: ASTNode = args[1].clone();
                cur.apply_arguments_inplace(&i).ok()?;
                cur.eval(ctx);
                acc.push(cur);
                Some(acc)
            })?,
        }
        .lift_ast(),
    )
}

fn blti_fseq(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    if args.iter_mut().all(|i| i.eval(ctx)) {
        Some(args.take().0.lift_ast())
    } else {
        None
    }
}

fn blti_include(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let args = &mut args.0;
    args[0].eval(ctx);
    let filename = args[0].conv_to_constant()?;
    let filename: &str = std::str::from_utf8(&filename).expect("got invalid include filename");
    Some(
        {
            cfg_if! {
                if #[cfg(feature = "compile")] {
                    match ctx.comp_map.get(Path::new(filename)).copied() {
                        None => crate::parser::file2ast(Path::new(filename), ctx.opts),
                        Some(compf) => ctx.load_from_compfile(&compf),
                    }
                } else {
                    crate::parser::file2ast(Path::new(filename), ctx.opts)
                }
            }
        }
        .expect("expected valid file")
        .lift_ast(),
    )
}

fn blti_lambda(args: &[ASTNode]) -> Option<ASTNode> {
    if args.len() < 2 {
        None
    } else {
        let largc: usize = atoi(&args[0].conv_to_constant()?).expect("expected number as argc");
        let body = Box::new(args[1..].to_vec().lift_ast().simplify());
        Some(ASTNode::Lambda { argc: largc, body })
    }
}

fn blti_lambda_lazy(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    if args.len() < 2 {
        None
    } else {
        let args = &mut args.0;
        let largc: usize = atoi(&unpack(&mut args[0], ctx)?).expect("expected number as argc");
        let body = Box::new(args[1..].to_vec().lift_ast().simplify());
        Some(ASTNode::Lambda { argc: largc, body })
    }
}

fn blti_lambda_strict(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let args = &mut args.0;
    if args.len() >= 2 && args.iter_mut().all(|i| i.eval(ctx)) {
        None
    } else {
        Some(ASTNode::Lambda {
            argc: atoi(&args[0].conv_to_constant()?).expect("expected number as argc"),
            body: Box::new(args[1..].to_vec().lift_ast().simplify()),
        })
    }
}

fn blti_pass(args: &[ASTNode]) -> Option<ASTNode> {
    Some(args.to_vec().lift_ast())
}
fn blti_suppress(_args: &[ASTNode]) -> Option<ASTNode> {
    Some(ASTNode::NullNode)
}

fn blti_undef(args: &mut CmdEvalArgs, ctx: &mut EvalContext<'_>) -> Option<ASTNode> {
    let varname = unpack(&mut args.0[0], ctx)?;
    ctx.defs.remove(&varname);
    Some(ASTNode::NullNode)
}

fn blti_une(args: &[ASTNode]) -> Option<ASTNode> {
    Some(
        args.iter()
            .cloned()
            .map(uneg)
            .collect::<Vec<_>>()
            .lift_ast(),
    )
}

fn blti_unee(args: &[ASTNode]) -> Option<ASTNode> {
    Some(
        CmdEvalArgs::from_wsdelim(
            args.iter()
                .cloned()
                .map(uneg)
                .collect::<Vec<_>>()
                .simplify(),
        )
        .0
        .lift_ast(),
    )
}

fn eval_args(args: &mut CmdEvalArgs, ctx: &mut EvalContext) {
    *args = CmdEvalArgs(
        args.take()
            .into_iter()
            .flat_map(|mut i| {
                i.eval(ctx);
                if let ASTNode::Grouped {
                    typ: GroupType::Dissolving,
                    elems,
                } = i
                {
                    elems
                } else {
                    i.lift_ast()
                }
            })
            .collect(),
    );
}

fn eval_cmd(cmd: &mut VAN, args: &mut CmdEvalArgs, ctx: &mut EvalContext) -> Option<ASTNode> {
    // evaluate command name
    for i in cmd.iter_mut() {
        i.eval(ctx);
    }
    // allow partial evaluation of command name
    *cmd = cmd.take().simplify().compact_toplevel();
    match cmd.clone().lift_ast().simplify() {
        ASTNode::Constant {
            non_space: true,
            data: cmd,
        } => {
            // evaluate command
            let cmd: &[u8] = &*cmd;
            if let Some((a, x)) = ctx.procdefs.get(cmd).copied() {
                if let BuiltInFn::Automatic(_) = &x {
                    eval_args(args, ctx);
                }
                match a {
                    Some(n) if args.len() != n => None,
                    _ => match x {
                        BuiltInFn::Manual(y) => y(args, ctx),
                        BuiltInFn::Automatic(y) => y(&args.0),
                    },
                }
            } else {
                let (n, mut x) = ctx.defs.get(cmd)?.clone();
                eval_args(args, ctx);
                if args.len() != n || x.apply_arguments_inplace(args).is_err() {
                    None
                } else {
                    Some(x)
                }
            }
        }
        ASTNode::Lambda { argc, mut body } => {
            eval_args(args, ctx);
            if args.len() != argc || body.apply_arguments_inplace(args).is_err() {
                None
            } else {
                Some(*body)
            }
        }
        _ => None,
    }
}

trait Eval {
    /// if (return value): fully evaluated
    fn eval(&mut self, ctx: &mut EvalContext) -> bool;
}

impl Eval for ASTNode {
    fn eval(mut self: &mut Self, ctx: &mut EvalContext) -> bool {
        use ASTNode::*;
        match &mut self {
            CmdEval { cmd, args } => {
                if let Some(x) = eval_cmd(cmd, args, ctx) {
                    *self = x;
                    true
                } else {
                    false
                }
            }
            Grouped { elems, .. } => elems.eval(ctx),
            _ => true,
        }
    }
}

impl Eval for [ASTNode] {
    fn eval(&mut self, ctx: &mut EvalContext) -> bool {
        let mut ret = true;
        for i in self.iter_mut() {
            ret &= i.eval(ctx);
        }
        ret
    }
}

impl Eval for CmdEvalArgs {
    fn eval(&mut self, ctx: &mut EvalContext) -> bool {
        self.0.eval(ctx)
    }
}

impl<'a> EvalContext<'a> {
    #[inline]
    pub fn new(opts: ParserOptions, comp_map: CompilatesMap<'a>) -> Self {
        Self {
            defs: HashMap::new(),
            procdefs: BUILTINS.clone(),
            opts,
            comp_map,
            _non_exhaustive: PhantomData,
        }
    }
}

pub fn eval(data: &mut VAN, ctx: &mut EvalContext<'_>, _comp_out: Option<&std::path::Path>) {
    let mut cplx = data.get_complexity();
    loop {
        data.eval(ctx);
        *data = data.take().simplify().compact_toplevel();
        let new_cplx = data.get_complexity();
        if new_cplx == cplx {
            break;
        }
        cplx = new_cplx;
    }
    cfg_if! {
        if #[cfg(feature = "compile")] {
            if let Some(comp_out) = _comp_out {
                ctx.save_to_compfile(comp_out, &*data)
                    .expect("save failed");
            }
        }
    }
}
