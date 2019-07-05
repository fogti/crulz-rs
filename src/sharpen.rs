extern crate boolinator;

pub struct TwoVec<T> {
    pub parts: Vec<Vec<T>>,
    last: Vec<T>,
}

impl<T> TwoVec<T> {
    fn take<TT>(mut x: &mut Vec<TT>) -> Vec<TT> {
        std::mem::replace(&mut x, vec![])
    }

    pub fn finish(&mut self) -> Vec<Vec<T>> {
        self.up_push();
        Self::take(&mut self.parts)
    }

    pub fn up_push(&mut self) {
        let tmp = Self::take(&mut self.last);
        if !tmp.is_empty() {
            self.parts.push(tmp);
        }
    }

    pub fn push(&mut self, x: T) {
        self.last.push(x);
    }
}

impl<T> Default for TwoVec<T> {
    fn default() -> Self {
        Self {
            parts: vec![],
            last: vec![],
        }
    }
}

pub struct ClassifyIT<'a, TT: 'a, TC, FnT, IT>
where
    TC: Copy + Default + std::cmp::PartialEq,
    FnT: FnMut(&TT) -> TC,
    IT: Iterator<Item = TT>,
{
    inner: &'a mut IT,
    fnx: FnT,
    edge: (Option<TC>, Option<TT>),
}

impl<'a, TT: 'a, TC, FnT, IT> ClassifyIT<'a, TT, TC, FnT, IT>
where
    TC: Copy + Default + std::cmp::PartialEq,
    FnT: FnMut(&TT) -> TC,
    IT: Iterator<Item = TT>,
{
    pub fn new(inner: &'a mut IT, fnx: FnT) -> Self {
        Self {
            inner,
            fnx,
            edge: (Some(Default::default()), None),
        }
    }
}

impl<'a, TT: 'a, TC, FnT, IT> std::iter::Iterator for ClassifyIT<'a, TT, TC, FnT, IT>
where
    TC: Copy + Default + std::cmp::PartialEq,
    FnT: FnMut(&TT) -> TC,
    IT: Iterator<Item = TT>,
{
    type Item = (TC, Vec<TT>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut ccl = self.edge.0?;
        let mut last = Vec::<TT>::new();

        if let Some(x) = self.edge.1.take() {
            last.push(x);
        }
        let fnx = &mut self.fnx;
        for (new_ccl, x) in self.inner.map(|x| {
            let fnr = fnx(&x);
            (fnr, x)
        }) {
            if new_ccl != ccl {
                if last.is_empty() {
                    ccl = new_ccl;
                    last.push(x);
                } else {
                    self.edge = (Some(new_ccl), Some(x));
                    return Some((ccl, last));
                }
            } else {
                last.push(x);
            }
        }

        // we reached the end of the inner iterator
        self.edge = (None, None);
        if last.is_empty() {
            None
        } else {
            Some((ccl, last))
        }
    }
}

pub trait Classify<'a, TT: 'a>
where
    Self: Sized + Iterator<Item = TT> + 'a,
{
    fn classify<TC, FnT>(&'a mut self, fnx: FnT) -> ClassifyIT<'a, TT, TC, FnT, Self>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC;
}

impl<'a, IT, TT: 'a> Classify<'a, TT> for IT
where
    Self: Sized + Iterator<Item = TT> + 'a,
{
    fn classify<TC, FnT>(&'a mut self, fnx: FnT) -> ClassifyIT<'a, TT, TC, FnT, Self>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC,
    {
        ClassifyIT::new(self, fnx)
    }
}

pub fn classify<'a, Input, FnT, TT: 'a, TC, TRes>(input: Input, fnx: FnT) -> TRes
where
    Input: IntoIterator<Item = TT>,
    FnT: FnMut(&TT) -> TC,
    TC: Copy + Default + PartialEq,
    TRes: std::iter::FromIterator<(TC, Vec<TT>)>,
{
    input.into_iter().classify(fnx).collect()
}

pub fn classify_as_vec<'a, Input, FnT, TT: 'a, TC>(input: Input, fnx: FnT) -> Vec<(TC, Vec<TT>)>
where
    Input: IntoIterator<Item = TT>,
    FnT: FnMut(&TT) -> TC,
    TC: Copy + Default + PartialEq,
{
    classify(input, fnx)
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate test;

    #[test]
    fn test_clsf0() {
        let input: Vec<u8> = vec![0, 0, 1, 1, 2, 2, 3, 0, 5, 5, 5];
        let res: Vec<_> = classify(input, |&curc| curc);
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
        let res: Vec<_> = classify(input, |curo| curo.is_some());
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
        let res: Vec<_> = classify(input, |curo| curo.is_some());
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

    #[test]
    fn test_clsfit2() {
        let input: Vec<Option<Vec<u8>>> = vec![
            Some(vec![0, 0, 1]),
            Some(vec![0, 1]),
            None,
            None,
            Some(vec![2]),
            None,
        ];
        let res =
            ClassifyIT::new(&mut input.into_iter(), |curo| curo.is_some()).collect::<Vec<_>>();
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

    #[bench]
    fn bench_clsfit2(b: &mut test::Bencher) {
        let input: Vec<Option<Vec<u8>>> = vec![
            Some(vec![0, 0, 1]),
            Some(vec![0, 1]),
            None,
            None,
            Some(vec![2]),
            None,
        ];
        b.iter(|| classify_as_vec(input.clone(), |curo| curo.is_some()));
    }
}
