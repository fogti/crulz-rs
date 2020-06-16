use crate::ast::{ASTNode, CmdEvalArgs, GroupType, VAN};

// === parser options

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ParserOptions {
    escc: u8,
    pass_escc: bool,
}

impl ParserOptions {
    #[inline]
    pub fn new(escc: u8, pass_escc: bool) -> Self {
        Self { escc, pass_escc }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum ParserErrorDetail {
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("got empty/invalid eval statement")]
    InvalidEval,
    #[error("expected '{}' instead", char::from(*.0))]
    ExpectedInstead(u8),

    /// Escaped end-of-scope markers are dangerous, because they probably don't
    /// do, what you would naively expect. The correct way to escape them is
    /// to *not* escape the corresponding begin-of-scope marker, which thus
    /// constitutes a Group.
    #[error("dangerous escaped end-of-scope marker ('{}')", char::from(*.0))]
    DangerousEos(u8),
    #[error("unexpected unbalanced end-of-scope marker ('{}')", char::from(*.0))]
    UnbalancedEos(u8),
}

type PED = ParserErrorDetail;

pub struct ParserError<'a> {
    origin: &'a [u8],
    offending: &'a [u8],
    detail: PED,
}

// === parse trait

trait Parse: Sized {
    /// # Return value
    /// * `Ok(rest, parsed_obj)`
    /// * `Err(offending_code, description)`
    fn parse(data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>>;
}

// === parser utils

fn get_offset_of<T>(whole_buffer: &T, part: &T) -> usize
where
    T: AsRef<[u8]> + ?Sized,
{
    // NOTE: use offset_from() once it's stable
    part.as_ref().as_ptr() as usize - whole_buffer.as_ref().as_ptr() as usize
}

fn str_slice_between<'a>(whole_buffer_start: &'a [u8], post_part: &'a [u8]) -> &'a [u8] {
    &whole_buffer_start[..get_offset_of(whole_buffer_start, post_part)]
}

static SCOPE_MARKERS: phf::Map<u8, (u8, GroupType)> = phf::phf_map! {
    b'(' => (b')', GroupType::Strict),
    b'{' => (b'}', GroupType::Loose),
};

fn is_scope_end(x: &u8) -> bool {
    SCOPE_MARKERS.values().any(|(eogm, _)| eogm == x)
}

/// 1. part while f(x) == true, then 2. part
fn str_split_at_while(x: &[u8], f: impl FnMut(&u8) -> bool) -> (&[u8], &[u8]) {
    x.split_at(x.iter().copied().take_while(f).count())
}

/// escaped escape symbol or other escaped code: optional passthrough
fn parse_escaped_const(i: u8, opts: ParserOptions) -> Option<ASTNode> {
    match i {
        b'{' | b'}' | b'$' => {}
        b'\n' => return Some(ASTNode::NullNode),
        _ if i != opts.escc || opts.pass_escc => return None,
        _ => {}
    }
    Some(ASTNode::Constant(true, vec![i].into()))
}

fn str_split_at_ctrl(
    data: &[u8],
    opts: ParserOptions,
    f_do_cont_at: impl Fn(&u8) -> bool,
) -> (&[u8], &[u8]) {
    str_split_at_while(data, |i| match i {
        b'$' | b'(' | b')' | b'{' | b'}' => false,
        _ => *i != opts.escc && f_do_cont_at(i),
    })
}

fn do_expect<'a>(origin: &'a [u8], rest: &'a [u8], c: u8) -> Result<&'a [u8], ParserError<'a>> {
    if rest.get(0) == Some(&c) {
        Ok(&rest[1..])
    } else {
        Err(ParserError {
            origin,
            offending: rest,
            detail: PED::ExpectedInstead(c),
        })
    }
}

impl Parse for ASTNode {
    fn parse(data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>> {
        let escc = opts.escc;
        let mut iter = data.iter();

        let i = *iter.next().ok_or_else(|| ParserError {
            origin: data,
            offending: data,
            detail: PED::UnexpectedEof,
        })?;
        match i {
            _ if i == escc => {
                let i = *iter.next().ok_or_else(|| ParserError {
                    origin: data,
                    offending: data,
                    detail: PED::UnexpectedEof,
                })?;
                if i == b'(' {
                    // got begin of cmdeval block
                    let (rest, mut vanx) = VAN::parse(iter.as_slice(), opts)?;
                    if vanx.is_empty() {
                        return Err(ParserError {
                            origin: data,
                            offending: &data[..std::cmp::min(data.len(), 3)],
                            detail: PED::InvalidEval,
                        });
                    }
                    let rest = do_expect(data, rest, /*(*/ b')')?;

                    // extract command
                    assert!(!vanx.is_empty());
                    let split_point = vanx
                        .iter()
                        .enumerate()
                        .filter_map(|y| if y.1.is_space() { Some(y.0 + 1) } else { None })
                        .next()
                        .unwrap_or(1);
                    let van = vanx.split_off(split_point);
                    let mut cmd = vanx;
                    if cmd.last().unwrap().is_space() {
                        cmd.pop();
                    }
                    Ok((rest, ASTNode::CmdEval(cmd, CmdEvalArgs::from_wsdelim(van))))
                } else if let Some(c) = parse_escaped_const(i, opts) {
                    Ok((iter.as_slice(), c))
                } else if is_scope_end(&i) {
                    Err(ParserError {
                        origin: data,
                        offending: str_slice_between(data, iter.as_slice()),
                        detail: PED::DangerousEos(i),
                    })
                } else {
                    // interpret it as a command (LaTeX-alike)
                    let (cmd, mut rest) =
                        str_split_at_ctrl(&data[1..], opts, |x| !x.is_ascii_whitespace());
                    if cmd.is_empty() {
                        return Err(ParserError {
                            origin: data,
                            offending: str_slice_between(data, iter.as_slice()),
                            detail: PED::InvalidEval,
                        });
                    }
                    let vanx = if rest.get(0) == Some(&b'(') {
                        let (tmp_rest, van) = VAN::parse(&rest[1..], opts)?;
                        rest = do_expect(data, tmp_rest, b')')?;
                        CmdEvalArgs::from_wsdelim(van)
                    } else {
                        Default::default()
                    };
                    Ok((
                        rest,
                        ASTNode::CmdEval(vec![ASTNode::Constant(true, cmd.into())], vanx),
                    ))
                }
            }
            b'$' => {
                let (cdat, rest) = str_split_at_while(iter.as_slice(), |&i| i == b'$');
                let (idxs, rest) = str_split_at_while(rest, u8::is_ascii_digit);
                Ok((
                    rest,
                    ASTNode::Argument {
                        indirection: cdat.len(),
                        index: atoi::atoi(idxs),
                    },
                ))
            }
            _ if is_scope_end(&i) => Err(ParserError {
                origin: data,
                offending: str_slice_between(data, iter.as_slice()),
                detail: PED::UnbalancedEos(i),
            }),
            _ => Ok(if let Some(&(eogm, rgt)) = SCOPE_MARKERS.get(&i) {
                let (rest, van) = VAN::parse(iter.as_slice(), opts)?;
                (do_expect(data, rest, eogm)?, ASTNode::Grouped(rgt, van))
            } else {
                let is_whitespace = i.is_ascii_whitespace();
                let (cdat, rest) =
                    str_split_at_ctrl(data, opts, |x| x.is_ascii_whitespace() == is_whitespace);
                (rest, ASTNode::Constant(!is_whitespace, cdat.into()))
            }),
        }
    }
}

impl Parse for VAN {
    fn parse(mut data: &[u8], opts: ParserOptions) -> Result<(&[u8], Self), ParserError<'_>> {
        let mut ret = VAN::new();
        while data.get(0).map(is_scope_end) == Some(false) {
            let (rest, node) = ASTNode::parse(data, opts)?;
            ret.push(node);
            data = rest;
        }
        Ok((data, ret))
    }
}

// === main parser

/// At top level, only parse things inside CmdEval's
pub fn parse_toplevel(mut data: &[u8], opts: ParserOptions) -> Result<VAN, ParserError<'_>> {
    let mut ret = VAN::new();
    while !data.is_empty() {
        let mut cstp_has_nws = false;
        let (cstp, rest) = str_split_at_while(data, |&i| {
            cstp_has_nws |= !i.is_ascii_whitespace();
            i != opts.escc
        });
        if !cstp.is_empty() {
            ret.push(ASTNode::Constant(cstp_has_nws, cstp.into()));
        }
        if rest.is_empty() {
            break;
        }
        let (rest, node) = ASTNode::parse(rest, opts)?;
        ret.push(node);
        data = rest;
    }
    Ok(ret)
}

fn print_error_nonutf8(
    stdstream: &mut termcolor::StandardStreamLock,
    filename: &std::path::Path,
    input: &[u8],
    e: &ParserError,
) -> std::io::Result<()> {
    use std::io::Write;
    use termcolor::{Color, ColorSpec, WriteColor};
    let (mut x_bold, mut color_red, mut x_note) =
        (ColorSpec::new(), ColorSpec::new(), ColorSpec::new());
    x_bold.set_bold(true);
    color_red.set_fg(Some(Color::Red)).set_bold(true);
    x_note.set_fg(Some(Color::Blue)).set_bold(true);
    let (x_bold, color_red, x_note) = (x_bold, color_red, x_note);
    stdstream.set_color(&x_bold)?;
    write!(stdstream, "crulz: ")?;
    stdstream.set_color(&color_red)?;
    write!(stdstream, "error")?;
    stdstream.set_color(&x_bold)?;
    writeln!(
        stdstream,
        ": {}: file contains non-UTF-8 data",
        filename.display()
    )?;

    stdstream.set_color(&x_bold)?;
    write!(stdstream, "crulz: ")?;
    stdstream.set_color(&color_red)?;
    write!(stdstream, "error")?;
    stdstream.set_color(&x_bold)?;
    let start_pos = get_offset_of(input, e.offending);
    writeln!(
        stdstream,
        ": {}: {}..{}: {}",
        filename.display(),
        start_pos,
        start_pos + e.offending.len(),
        e.detail
    )?;

    stdstream.set_color(&x_bold)?;
    write!(stdstream, "crulz: ")?;
    stdstream.set_color(&x_note)?;
    write!(stdstream, "note")?;
    stdstream.set_color(&x_bold)?;
    let start_pos = get_offset_of(input, e.origin);
    writeln!(
        stdstream,
        ": {}: error origin is at position {}",
        filename.display(),
        start_pos,
    )?;

    stdstream.reset()?;

    Ok(())
}

pub fn file2ast(filename: &std::path::Path, opts: ParserOptions) -> Result<VAN, anyhow::Error> {
    use anyhow::Context;

    let fh = readfilez::read_from_file(std::fs::File::open(filename))
        .with_context(|| format!("unable to read file '{}'", filename.display()))?;
    let input = fh.as_slice();

    parse_toplevel(input, opts).map_err(|e| {
        use std::str::FromStr;
        let stdstream = termcolor::StandardStream::stderr(
            codespan_reporting::term::ColorArg::from_str("auto")
                .unwrap()
                .into(),
        );
        let mut stdstream = stdstream.lock();

        if let Ok(input) = std::str::from_utf8(input) {
            use codespan_reporting::{
                diagnostic::{Diagnostic, Label},
                term,
            };

            let mut files = codespan::Files::new();
            let fileid = files.add(filename, input);
            let start_pos = get_offset_of(input.as_bytes(), e.offending);
            let start_pos_origin = get_offset_of(input.as_bytes(), e.origin);

            term::emit(
                &mut stdstream,
                &term::Config::default(),
                &files,
                &Diagnostic::error()
                    .with_message(e.detail.to_string())
                    .with_labels(vec![
                        Label::primary(fileid, start_pos..(start_pos + e.offending.len())),
                        Label::secondary(fileid, start_pos_origin..start_pos),
                    ]),
            )
            .unwrap();
        } else {
            print_error_nonutf8(&mut stdstream, filename, input, &e).unwrap();
        }
        anyhow::anyhow!("{}", e.detail)
    })
}
