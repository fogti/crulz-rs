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

pub struct ClassifyIT<'a, TT, TC, FnT, IT>
where
    TT: Clone,
    TC: Copy + Default + std::cmp::PartialEq,
    FnT: FnMut(&TT) -> TC,
    IT: Iterator<Item = TT>,
{
    inner: &'a mut IT,
    fnx: FnT,
    edge: (Option<TC>, Option<TT>),
}

impl<'a, TT, TC, FnT, IT> ClassifyIT<'a, TT, TC, FnT, IT>
where
    TT: Clone + 'a,
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

impl<'a, TT, TC, FnT, IT> std::iter::Iterator for ClassifyIT<'a, TT, TC, FnT, IT>
where
    TT: Clone + 'a,
    TC: Copy + Default + std::cmp::PartialEq,
    FnT: FnMut(&TT) -> TC,
    IT: Iterator<Item = TT>,
{
    type Item = (TC, Vec<TT>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut ccl = self.edge.0?;
        let mut last = Vec::<TT>::new();

        if let Some(x) = &self.edge.1 {
            last.push(x.clone());
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

pub trait Classify<'a, TT>
where
    TT: Clone + 'a,
{
    // This function splits the input(self) at every change of the return value of fnx
    // signature of fnx := fn fnx(ccl: u32, curc: u8) -> u32 (new ccl)
    // This function is a special variant of the TwoVec methods
    fn classify<TC, FnT>(self, fnx: FnT) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC;
}

pub trait ClassifyIter<'a, TT>
where
    Self: Sized + Iterator<Item = TT> + 'a,
    TT: Clone + 'a,
{
    fn classify_iter<TC, FnT>(&'a mut self, fnx: FnT) -> ClassifyIT<'a, TT, TC, FnT, Self>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC;
}

impl<'a, InT, ITT, TT> Classify<'a, TT> for InT
where
    InT: IntoIterator<Item = ITT>,
    ITT: std::ops::Deref<Target = TT> + 'a,
    TT: Clone + 'a,
{
    fn classify<TC, FnT>(self, fnx: FnT) -> Vec<(TC, Vec<TT>)>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC,
    {
        self.into_iter()
            .map(|i| i.deref().clone())
            .classify_iter(fnx)
            .collect()
    }
}

impl<'a, IT, TT> ClassifyIter<'a, TT> for IT
where
    Self: Sized + Iterator<Item = TT> + 'a,
    TT: Clone + 'a,
{
    fn classify_iter<TC, FnT>(&'a mut self, fnx: FnT) -> ClassifyIT<'a, TT, TC, FnT, Self>
    where
        TC: Copy + Default + std::cmp::PartialEq,
        FnT: FnMut(&TT) -> TC,
    {
        ClassifyIT::new(self, fnx)
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
}
