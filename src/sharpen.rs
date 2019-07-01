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

pub trait Classify<TT>
where
    TT: Clone,
{
    // This function splits the input(self) at every change of the return value of fnx
    // signature of fnx := fn fnx(ccl: u32, curc: u8) -> u32 (new ccl)
    // This function is a special variant of the TwoVec methods
    fn classify<TC, FnT>(self, fnx: FnT) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC;
}

impl<InT, ITT, TT> Classify<TT> for InT
where
    InT: IntoIterator<Item = ITT>,
    ITT: std::ops::Deref<Target = TT>,
    TT: Clone,
{
    fn classify<TC, FnT>(self, mut fnx: FnT) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC,
    {
        let mut parts = Vec::<(TC, Vec<TT>)>::new();
        let start_ccl: TC = Default::default();
        let mut last = (start_ccl, Vec::<TT>::new());
        let mut ccl = start_ccl;

        for i in self
            .into_iter()
            .map(|x| {
                let new_ccl = fnx(&x);
                let is_change = new_ccl != ccl;
                ccl = new_ccl;
                use boolinator::Boolinator;
                (is_change.as_some(new_ccl), Some(x.deref().clone()))
            })
            .chain(vec![(Some(start_ccl), None as Option<TT>)].into_iter())
        {
            let (pccl, pcurc) = i;

            if let Some(x) = pccl {
                let mut tmp = std::mem::replace(&mut last, (x, vec![]));
                if !tmp.1.is_empty() {
                    tmp.1.shrink_to_fit();
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
        let res = input.classify(|&curc| curc);
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

    #[test]
    fn test_clsf1() {
        let input: Vec<Option<u8>> = vec![
            Some(0),
            Some(1),
            Some(5),
            Some(5),
            None,
            None,
            Some(0),
            None,
        ];
        let res = input.classify(|curo| curo.is_some());
        assert_eq!(
            res,
            vec![
                (true, vec![Some(0), Some(1), Some(5), Some(5)]),
                (false, vec![None, None]),
                (true, vec![Some(0)]),
                (false, vec![None]),
            ]
        );
    }

    #[test]
    fn test_clsf2() {
        let input: Vec<Option<Vec<u8>>> = vec![
            Some(vec![0, 0, 1]),
            Some(vec![0, 1]),
            None,
            None,
            Some(vec![2]),
            None,
        ];
        let res = input.classify(|curo| curo.is_some());
        assert_eq!(
            res,
            vec![
                (true, vec![Some(vec![0, 0, 1]), Some(vec![0, 1])]),
                (false, vec![None, None]),
                (true, vec![Some(vec![2])]),
                (false, vec![None]),
            ]
        );
    }
}
