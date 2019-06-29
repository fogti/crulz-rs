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

pub trait Classify<'a, TT>
where
    TT: Copy + 'a,
{
    // This function splits the input(self) at every change of the return value of fnx
    // signature of fnx := fn fnx(ccl: u32, curc: u8) -> u32 (new ccl)
    // This function is a special variant of the TwoVec methods
    fn classify<TC, FnT>(self, fnx: FnT, start_ccl: TC) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + std::cmp::PartialEq,
        FnT: Fn(TC, TT) -> TC;
}

impl<'a, InT, ITT, TT> Classify<'a, TT> for InT
where
    InT: IntoIterator<Item = ITT>,
    ITT: std::ops::Deref<Target = TT> + 'a,
    TT: Copy + 'a,
{
    fn classify<TC, FnT>(self, fnx: FnT, start_ccl: TC) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + std::cmp::PartialEq,
        FnT: Fn(TC, TT) -> TC,
    {
        let mut parts = Vec::<(TC, Vec<TT>)>::new();
        let mut last = (start_ccl, Vec::<TT>::new());
        let mut ccl: TC = start_ccl;

        for i in self
            .into_iter()
            .map(|x| {
                let x = *x;
                let new_ccl = fnx(ccl, x);
                let is_change = new_ccl != ccl;
                ccl = new_ccl;
                use boolinator::Boolinator;
                (is_change.as_some(new_ccl), Some(x))
            })
            .chain(vec![(Some(start_ccl), None as Option<TT>)].into_iter())
        {
            let (pccl, pcurc) = i;

            if let Some(x) = pccl {
                let tmp = std::mem::replace(&mut last, (x, vec![]));
                if !tmp.1.is_empty() {
                    parts.push(tmp);
                }
            }
            if let Some(x) = pcurc {
                last.1.push(x);
            }
        }

        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clsf0() {
        let input: Vec<u8> = vec![0, 0, 1, 1, 2, 2, 3, 0, 5, 5, 5];
        let res = input.classify(|_ocl, curc| curc, 0);
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
