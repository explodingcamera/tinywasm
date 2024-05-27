use crate::unlikely;
use alloc::{borrow::Cow, boxed::Box, vec};
use core::ops::RangeBounds;

// A Vec-like type that doesn't deallocate memory when popping elements.
#[derive(Debug)]
pub(crate) struct BoxVec<T> {
    pub(crate) data: Box<[T]>,
    pub(crate) end: usize,
}

impl<T: Copy + Default> BoxVec<T> {
    #[inline(always)]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self { data: vec![T::default(); capacity].into_boxed_slice(), end: 0 }
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) {
        assert!(self.end <= self.data.len(), "stack overflow");
        self.data[self.end] = value;
        self.end += 1;
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Option<T> {
        assert!(self.end <= self.data.len(), "invalid stack state (should be impossible)");
        if unlikely(self.end == 0) {
            None
        } else {
            self.end -= 1;
            Some(self.data[self.end])
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.end
    }

    #[inline(always)]
    pub(crate) fn extend_from_slice(&mut self, values: &[T]) {
        let new_end = self.end + values.len();
        assert!(new_end <= self.data.len(), "stack overflow");
        self.data[self.end..new_end].copy_from_slice(values);
        self.end = new_end;
    }

    #[inline(always)]
    pub(crate) fn last_mut(&mut self) -> Option<&mut T> {
        assert!(self.end <= self.data.len(), "invalid stack state (should be impossible)");
        if unlikely(self.end == 0) {
            None
        } else {
            Some(&mut self.data[self.end - 1])
        }
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> Option<&T> {
        assert!(self.end <= self.data.len(), "invalid stack state (should be impossible)");
        if unlikely(self.end == 0) {
            None
        } else {
            Some(&self.data[self.end - 1])
        }
    }

    #[inline(always)]
    pub(crate) fn drain(&mut self, range: impl RangeBounds<usize>) -> Cow<'_, [T]> {
        let start = match range.start_bound() {
            core::ops::Bound::Included(&start) => start,
            core::ops::Bound::Excluded(&start) => start + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            core::ops::Bound::Included(&end) => end + 1,
            core::ops::Bound::Excluded(&end) => end,
            core::ops::Bound::Unbounded => self.end,
        };

        assert!(start <= end);
        assert!(end <= self.end);

        if end == self.end {
            self.end = start;
            return Cow::Borrowed(&self.data[start..end]);
        }

        let drain = self.data[start..end].to_vec();
        self.data.copy_within(end..self.end, start);
        self.end -= end - start;
        Cow::Owned(drain)
    }
}

impl<T> core::ops::Index<usize> for BoxVec<T> {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: usize) -> &T {
        &self.data[index]
    }
}

impl<T> core::ops::Index<core::ops::Range<usize>> for BoxVec<T> {
    type Output = [T];

    #[inline(always)]
    fn index(&self, index: core::ops::Range<usize>) -> &[T] {
        &self.data[index]
    }
}

impl<T> core::ops::IndexMut<usize> for BoxVec<T> {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut self.data[index]
    }
}
