extern crate boolinator;

pub struct TwoVec<T> {
    pub parts: Vec<Vec<T>>,
    last: Vec<T>,
}

impl<T> TwoVec<T> {
    pub fn new() -> Self {
        Self {
            parts: vec![],
            last: vec![],
        }
    }

    pub fn finish(&mut self) -> Vec<Vec<T>> {
        self.up_push();
        std::mem::replace(&mut self.parts, vec![])
    }

    pub fn up_push(&mut self) {
        let tmp = std::mem::replace(&mut self.last, vec![]);
        if !tmp.is_empty() {
            self.parts.push(tmp);
        }
    }

    pub fn push(&mut self, x: T) {
        self.last.push(x);
    }
}

struct ClassifyHelper<TC, TT>
where
    TC: Copy,
{
    pub parts: Vec<(TC, Vec<TT>)>,
    last: (TC, Vec<TT>),
    start_ccl: TC,
}

impl<TC, TT> ClassifyHelper<TC, TT>
where
    TC: Copy,
{
    pub fn new(start_ccl: TC) -> Self {
        Self {
            parts: vec![],
            last: (start_ccl, vec![]),
            start_ccl,
        }
    }

    pub fn finish(mut self) -> Vec<(TC, Vec<TT>)> {
        self.up_push(self.start_ccl);
        self.parts
    }

    fn up_push(&mut self, ccl: TC) {
        let tmp = std::mem::replace(&mut self.last, (ccl, vec![]));
        if !tmp.1.is_empty() {
            self.parts.push(tmp);
        }
    }

    pub fn xpush(&mut self, x: TT, pccl: Option<TC>) {
        if let Some(x) = pccl {
            self.up_push(x);
        }
        self.last.1.push(x);
    }
}

// This function splits the bytestring at every change of the return value of fnx
// signature of fnx := fn fnx(ccl: u32, curc: u8) -> u32 (new ccl)
// This function is a special variant of the TwoVec methods
pub fn classify_bstr<FnT, TC, TT>(input: Vec<TT>, fnx: FnT, start_ccl: TC) -> Vec<(TC, Vec<TT>)>
where
    FnT: Fn(TC, TT) -> TC,
    TC: Copy + std::cmp::PartialEq,
    TT: Copy,
{
    let x2: Vec<_> = {
        let mut ccl: TC = start_ccl;
        input
            .into_iter()
            .map(|x| {
                let new_ccl = fnx(ccl, x);
                let ret = (new_ccl, x, (new_ccl != ccl));
                if new_ccl != ccl {
                    ccl = new_ccl;
                }
                ret
            })
            .collect()
    };

    let mut helper = ClassifyHelper::<TC, TT>::new(start_ccl);

    for i in x2 {
        use boolinator::Boolinator;
        let (ccl, curc, is_change) = i;
        helper.xpush(curc, is_change.as_some(ccl));
    }

    helper.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clsbs0() {
        let input: Vec<u8> = vec![0, 0, 1, 1, 2, 2, 3, 0, 5, 5, 5];
        let res = classify_bstr(input, |_ocl, curc| curc as u32, 0);
        assert_eq!(
            res,
            vec![
                (0, vec![0, 0]),
                (1, vec![1, 1]),
                (2, vec![2, 2]),
                (3, vec![3]),
                (0, vec![0]),
                (5, vec![5, 5, 5]),
            ]
        );
    }
}
