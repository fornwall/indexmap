//! A hash set implemented using `IndexMap`

#[cfg(feature = "rayon")]
pub use ::rayon::set as rayon;

use std::cmp::Ordering;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::iter::{FromIterator, Chain};
use std::hash::{Hash, BuildHasher};
use std::ops::RangeFull;
use std::ops::{BitAnd, BitOr, BitXor, Sub};
use std::slice;
use std::vec;

use super::{IndexMap, Equivalent, Entries};

type Bucket<T> = super::Bucket<T, ()>;

/// A hash set where the iteration order of the values is independent of their
/// hash values.
///
/// The interface is closely compatible with the standard `HashSet`, but also
/// has additional features.
///
/// # Order
///
/// The values have a consistent order that is determined by the sequence of
/// insertion and removal calls on the set. The order does not depend on the
/// values or the hash function at all. Note that insertion order and value
/// are not affected if a re-insertion is attempted once an element is
/// already present.
///
/// All iterators traverse the set *in order*.  Set operation iterators like
/// `union` produce a concatenated order, as do their matching "bitwise"
/// operators.  See their documentation for specifics.
///
/// The insertion order is preserved, with **notable exceptions** like the
/// `.remove()` or `.swap_remove()` methods. Methods such as `.sort_by()` of
/// course result in a new order, depending on the sorting order.
///
/// # Indices
///
/// The values are indexed in a compact range without holes in the range
/// `0..self.len()`. For example, the method `.get_full` looks up the index for
/// a value, and the method `.get_index` looks up the value by index.
///
/// # Examples
///
/// ```
/// use indexmap::IndexSet;
///
/// // Collects which letters appear in a sentence.
/// let letters: IndexSet<_> = "a short treatise on fungi".chars().collect();
/// 
/// assert!(letters.contains(&'s'));
/// assert!(letters.contains(&'t'));
/// assert!(letters.contains(&'u'));
/// assert!(!letters.contains(&'y'));
/// ```
#[derive(Clone)]
pub struct IndexSet<T, S = RandomState> {
    map: IndexMap<T, (), S>,
}

impl<T, S> Entries for IndexSet<T, S> {
    type Entry = Bucket<T>;

    fn into_entries(self) -> Vec<Self::Entry> {
        self.map.into_entries()
    }

    fn as_entries(&self) -> &[Self::Entry] {
        self.map.as_entries()
    }

    fn as_entries_mut(&mut self) -> &mut [Self::Entry] {
        self.map.as_entries_mut()
    }

    fn with_entries<F>(&mut self, f: F)
        where F: FnOnce(&mut [Self::Entry])
    {
        self.map.with_entries(f);
    }
}

impl<T, S> fmt::Debug for IndexSet<T, S>
    where T: fmt::Debug + Hash + Eq,
          S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if cfg!(not(feature = "test_debug")) {
            f.debug_set().entries(self.iter()).finish()
        } else {
            // Let the inner `IndexMap` print all of its details
            f.debug_struct("IndexSet").field("map", &self.map).finish()
        }
    }
}

impl<T> IndexSet<T> {
    /// Create a new set. (Does not allocate.)
    pub fn new() -> Self {
        IndexSet { map: IndexMap::new() }
    }

    /// Create a new set with capacity for `n` elements.
    /// (Does not allocate if `n` is zero.)
    ///
    /// Computes in **O(n)** time.
    pub fn with_capacity(n: usize) -> Self {
        IndexSet { map: IndexMap::with_capacity(n) }
    }
}

impl<T, S> IndexSet<T, S> {
    /// Create a new set with capacity for `n` elements.
    /// (Does not allocate if `n` is zero.)
    ///
    /// Computes in **O(n)** time.
    pub fn with_capacity_and_hasher(n: usize, hash_builder: S) -> Self
        where S: BuildHasher
    {
        IndexSet { map: IndexMap::with_capacity_and_hasher(n, hash_builder) }
    }

    /// Return the number of elements in the set.
    ///
    /// Computes in **O(1)** time.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if the set contains no elements.
    ///
    /// Computes in **O(1)** time.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Create a new set with `hash_builder`
    pub fn with_hasher(hash_builder: S) -> Self
        where S: BuildHasher
    {
        IndexSet { map: IndexMap::with_hasher(hash_builder) }
    }

    /// Return a reference to the set's `BuildHasher`.
    pub fn hasher(&self) -> &S
        where S: BuildHasher
    {
        self.map.hasher()
    }

    /// Computes in **O(1)** time.
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }
}

impl<T, S> IndexSet<T, S>
    where T: Hash + Eq,
          S: BuildHasher,
{
    /// Remove all elements in the set, while preserving its capacity.
    ///
    /// Computes in **O(n)** time.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// FIXME Not implemented fully yet
    pub fn reserve(&mut self, additional: usize) {
        self.map.reserve(additional);
    }

    /// Insert the value into the set.
    ///
    /// If an equivalent item already exists in the set, it returns
    /// `false` leaving the original value in the set and without
    /// altering its insertion order. Otherwise, it inserts the new
    /// item and returns `true`.
    ///
    /// Computes in **O(1)** time (amortized average).
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }

    /// Insert the value into the set, and get its index.
    ///
    /// If an equivalent item already exists in the set, it returns
    /// the index of the existing item and `false`, leaving the
    /// original value in the set and without altering its insertion
    /// order. Otherwise, it inserts the new item and returns the index
    /// of the inserted item and `true`.
    ///
    /// Computes in **O(1)** time (amortized average).
    pub fn insert_full(&mut self, value: T) -> (usize, bool) {
        use super::map::Entry::*;

        match self.map.entry(value) {
            Occupied(e) => (e.index(), false),
            Vacant(e) => {
                let index = e.index();
                e.insert(());
                (index, true)
            }
        }
    }

    /// Return an iterator over the values of the set, in their order
    pub fn iter(&self) -> Iter<T> {
        Iter {
            iter: self.map.keys().iter
        }
    }

    /// Return an iterator over the values that are in `self` but not `other`.
    ///
    /// Values are produced in the same order that they appear in `self`.
    pub fn difference<'a, S2>(&'a self, other: &'a IndexSet<T, S2>) -> Difference<'a, T, S2>
        where S2: BuildHasher
    {
        Difference {
            iter: self.iter(),
            other: other,
        }
    }

    /// Return an iterator over the values that are in `self` or `other`,
    /// but not in both.
    ///
    /// Values from `self` are produced in their original order, followed by
    /// values from `other` in their original order.
    pub fn symmetric_difference<'a, S2>(&'a self, other: &'a IndexSet<T, S2>)
        -> SymmetricDifference<'a, T, S, S2>
        where S2: BuildHasher
    {
        SymmetricDifference {
            iter: self.difference(other).chain(other.difference(self)),
        }
    }

    /// Return an iterator over the values that are in both `self` and `other`.
    ///
    /// Values are produced in the same order that they appear in `self`.
    pub fn intersection<'a, S2>(&'a self, other: &'a IndexSet<T, S2>) -> Intersection<'a, T, S2>
        where S2: BuildHasher
    {
        Intersection {
            iter: self.iter(),
            other: other,
        }
    }

    /// Return an iterator over all values that are in `self` or `other`.
    ///
    /// Values from `self` are produced in their original order, followed by
    /// values that are unique to `other` in their original order.
    pub fn union<'a, S2>(&'a self, other: &'a IndexSet<T, S2>) -> Union<'a, T, S>
        where S2: BuildHasher
    {
        Union {
            iter: self.iter().chain(other.difference(self)),
        }
    }

    /// Return `true` if an equivalent to `value` exists in the set.
    ///
    /// Computes in **O(1)** time (average).
    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
        where Q: Hash + Equivalent<T>,
    {
        self.map.contains_key(value)
    }

    /// Return a reference to the value stored in the set, if it is present,
    /// else `None`.
    ///
    /// Computes in **O(1)** time (average).
    pub fn get<Q: ?Sized>(&self, value: &Q) -> Option<&T>
        where Q: Hash + Equivalent<T>,
    {
        self.map.get_full(value).map(|(_, x, &())| x)
    }

    /// Return item index and value
    pub fn get_full<Q: ?Sized>(&self, value: &Q) -> Option<(usize, &T)>
        where Q: Hash + Equivalent<T>,
    {
        self.map.get_full(value).map(|(i, x, &())| (i, x))
    }

    /// Adds a value to the set, replacing the existing value, if any, that is
    /// equal to the given one. Returns the replaced value.
    ///
    /// Computes in **O(1)** time (average).
    pub fn replace(&mut self, value: T) -> Option<T>
    {
        use super::map::Entry::*;

        match self.map.entry(value) {
            Vacant(e) => { e.insert(()); None },
            Occupied(e) => Some(e.replace_key()),
        }
    }

    /// FIXME Same as .swap_remove
    ///
    /// Computes in **O(1)** time (average).
    #[deprecated(note = "use `swap_remove` or `shift_remove`")]
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
        where Q: Hash + Equivalent<T>,
    {
        self.swap_remove(value)
    }

    /// Remove the value from the set, and return `true` if it was present.
    ///
    /// Like `Vec::swap_remove`, the value is removed by swapping it with the
    /// last element of the set and popping it off. **This perturbs
    /// the postion of what used to be the last element!**
    ///
    /// Return `false` if `value` was not in the set.
    ///
    /// Computes in **O(1)** time (average).
    pub fn swap_remove<Q: ?Sized>(&mut self, value: &Q) -> bool
        where Q: Hash + Equivalent<T>,
    {
        self.map.swap_remove(value).is_some()
    }

    /// Remove the value from the set, and return `true` if it was present.
    ///
    /// Like `Vec::remove`, the value is removed by shifting all of the
    /// elements that follow it, preserving their relative order.
    /// **This perturbs the index of all of those elements!**
    ///
    /// Return `false` if `value` was not in the set.
    ///
    /// Computes in **O(n)** time (average).
    pub fn shift_remove<Q: ?Sized>(&mut self, value: &Q) -> bool
        where Q: Hash + Equivalent<T>,
    {
        self.map.shift_remove(value).is_some()
    }

    /// FIXME Same as .swap_take
    ///
    /// Computes in **O(1)** time (average).
    pub fn take<Q: ?Sized>(&mut self, value: &Q) -> Option<T>
        where Q: Hash + Equivalent<T>,
    {
        self.swap_take(value)
    }

    /// Removes and returns the value in the set, if any, that is equal to the
    /// given one.
    ///
    /// Like `Vec::swap_remove`, the value is removed by swapping it with the
    /// last element of the set and popping it off. **This perturbs
    /// the postion of what used to be the last element!**
    ///
    /// Return `None` if `value` was not in the set.
    ///
    /// Computes in **O(1)** time (average).
    pub fn swap_take<Q: ?Sized>(&mut self, value: &Q) -> Option<T>
        where Q: Hash + Equivalent<T>,
    {
        self.map.swap_remove_full(value).map(|(_, x, ())| x)
    }

    /// Remove the value from the set return it and the index it had.
    ///
    /// Like `Vec::swap_remove`, the value is removed by swapping it with the
    /// last element of the set and popping it off. **This perturbs
    /// the postion of what used to be the last element!**
    ///
    /// Return `None` if `value` was not in the set.
    pub fn swap_remove_full<Q: ?Sized>(&mut self, value: &Q) -> Option<(usize, T)>
        where Q: Hash + Equivalent<T>,
    {
        self.map.swap_remove_full(value).map(|(i, x, ())| (i, x))
    }

    /// Remove the value from the set return it and the index it had.
    ///
    /// Like `Vec::remove`, the value is removed by shifting all of the
    /// elements that follow it, preserving their relative order.
    /// **This perturbs the index of all of those elements!**
    ///
    /// Return `None` if `value` was not in the set.
    pub fn shift_remove_full<Q: ?Sized>(&mut self, value: &Q) -> Option<(usize, T)>
        where Q: Hash + Equivalent<T>,
    {
        self.map.shift_remove_full(value).map(|(i, x, ())| (i, x))
    }

    /// Remove the last value
    ///
    /// Computes in **O(1)** time (average).
    pub fn pop(&mut self) -> Option<T> {
        self.map.pop().map(|(x, ())| x)
    }

    /// Scan through each value in the set and keep those where the
    /// closure `keep` returns `true`.
    ///
    /// The elements are visited in order, and remaining elements keep their
    /// order.
    ///
    /// Computes in **O(n)** time (average).
    pub fn retain<F>(&mut self, mut keep: F)
        where F: FnMut(&T) -> bool,
    {
        self.map.retain(move |x, &mut ()| keep(x))
    }

    /// Sort the set’s values by their default ordering.
    ///
    /// See `sort_by` for details.
    pub fn sort(&mut self)
        where T: Ord,
    {
        self.map.sort_keys()
    }

    /// Sort the set’s values in place using the comparison function `compare`.
    ///
    /// Computes in **O(n log n)** time and **O(n)** space. The sort is stable.
    pub fn sort_by<F>(&mut self, mut compare: F)
        where F: FnMut(&T, &T) -> Ordering,
    {
        self.map.sort_by(move |a, _, b, _| compare(a, b));
    }

    /// Sort the values of the set and return a by value iterator of
    /// the values with the result.
    ///
    /// The sort is stable.
    pub fn sorted_by<F>(self, mut cmp: F) -> IntoIter<T>
        where F: FnMut(&T, &T) -> Ordering
    {
        IntoIter {
            iter: self.map.sorted_by(move |a, &(), b, &()| cmp(a, b)).iter,
        }
    }

    /// Clears the `IndexSet`, returning all values as a drain iterator.
    /// Keeps the allocated memory for reuse.
    pub fn drain(&mut self, range: RangeFull) -> Drain<T> {
        Drain {
            iter: self.map.drain(range).iter,
        }
    }
}

impl<T, S> IndexSet<T, S> {
    /// Get a value by index
    ///
    /// Valid indices are *0 <= index < self.len()*
    ///
    /// Computes in **O(1)** time.
    pub fn get_index(&self, index: usize) -> Option<&T> {
        self.map.get_index(index).map(|(x, &())| x)
    }

    /// Remove the key-value pair by index
    ///
    /// Valid indices are *0 <= index < self.len()*
    ///
    /// Like `Vec::swap_remove`, the value is removed by swapping it with the
    /// last element of the set and popping it off. **This perturbs
    /// the postion of what used to be the last element!**
    ///
    /// Computes in **O(1)** time (average).
    pub fn swap_remove_index(&mut self, index: usize) -> Option<T> {
        self.map.swap_remove_index(index).map(|(x, ())| x)
    }

    /// Remove the key-value pair by index
    ///
    /// Valid indices are *0 <= index < self.len()*
    ///
    /// Like `Vec::remove`, the value is removed by shifting all of the
    /// elements that follow it, preserving their relative order.
    /// **This perturbs the index of all of those elements!**
    ///
    /// Computes in **O(n)** time (average).
    pub fn shift_remove_index(&mut self, index: usize) -> Option<T> {
        self.map.shift_remove_index(index).map(|(x, ())| x)
    }
}


/// An owning iterator over the items of a `IndexSet`.
///
/// This `struct` is created by the [`into_iter`] method on [`IndexSet`]
/// (provided by the `IntoIterator` trait). See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`into_iter`]: struct.IndexSet.html#method.into_iter
pub struct IntoIter<T> {
    iter: vec::IntoIter<Bucket<T>>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    iterator_methods!(Bucket::key);
}

impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(Bucket::key)
    }
}

impl<T> ExactSizeIterator for IntoIter<T> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl<T: fmt::Debug> fmt::Debug for IntoIter<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let iter = self.iter.as_slice().iter().map(Bucket::key_ref);
        f.debug_list().entries(iter).finish()
    }
}


/// An iterator over the items of a `IndexSet`.
///
/// This `struct` is created by the [`iter`] method on [`IndexSet`].
/// See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`iter`]: struct.IndexSet.html#method.iter
pub struct Iter<'a, T: 'a> {
    iter: slice::Iter<'a, Bucket<T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    iterator_methods!(Bucket::key_ref);
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(Bucket::key_ref)
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl<'a, T> Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Iter { iter: self.iter.clone() }
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for Iter<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}

/// A draining iterator over the items of a `IndexSet`.
///
/// This `struct` is created by the [`drain`] method on [`IndexSet`].
/// See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`drain`]: struct.IndexSet.html#method.drain
pub struct Drain<'a, T: 'a> {
    iter: vec::Drain<'a, Bucket<T>>,
}

impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;

    iterator_methods!(Bucket::key);
}

impl<'a, T> DoubleEndedIterator for Drain<'a, T> {
    double_ended_iterator_methods!(Bucket::key);
}

impl<'a, T, S> IntoIterator for &'a IndexSet<T, S>
    where T: Hash + Eq,
          S: BuildHasher,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, S> IntoIterator for IndexSet<T, S>
    where T: Hash + Eq,
          S: BuildHasher,
{
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            iter: self.map.into_iter().iter,
        }
    }
}

impl<T, S> FromIterator<T> for IndexSet<T, S>
    where T: Hash + Eq,
          S: BuildHasher + Default,
{
    fn from_iter<I: IntoIterator<Item=T>>(iterable: I) -> Self {
        let iter = iterable.into_iter().map(|x| (x, ()));
        IndexSet { map: IndexMap::from_iter(iter) }
    }
}

impl<T, S> Extend<T> for IndexSet<T, S>
    where T: Hash + Eq,
          S: BuildHasher,
{
    fn extend<I: IntoIterator<Item=T>>(&mut self, iterable: I) {
        let iter = iterable.into_iter().map(|x| (x, ()));
        self.map.extend(iter);
    }
}

impl<'a, T, S> Extend<&'a T> for IndexSet<T, S>
    where T: Hash + Eq + Copy,
          S: BuildHasher,
{
    fn extend<I: IntoIterator<Item=&'a T>>(&mut self, iterable: I) {
        let iter = iterable.into_iter().map(|&x| x);
        self.extend(iter);
    }
}


impl<T, S> Default for IndexSet<T, S>
    where S: BuildHasher + Default,
{
    /// Return an empty `IndexSet`
    fn default() -> Self {
        IndexSet { map: IndexMap::default() }
    }
}

impl<T, S1, S2> PartialEq<IndexSet<T, S2>> for IndexSet<T, S1>
    where T: Hash + Eq,
          S1: BuildHasher,
          S2: BuildHasher
{
    fn eq(&self, other: &IndexSet<T, S2>) -> bool {
        self.len() == other.len() && self.is_subset(other)
    }
}

impl<T, S> Eq for IndexSet<T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
}

impl<T, S> IndexSet<T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
    /// Returns `true` if `self` has no elements in common with `other`.
    pub fn is_disjoint<S2>(&self, other: &IndexSet<T, S2>) -> bool
        where S2: BuildHasher
    {
        if self.len() <= other.len() {
            self.iter().all(move |value| !other.contains(value))
        } else {
            other.iter().all(move |value| !self.contains(value))
        }
    }

    /// Returns `true` if all elements of `self` are contained in `other`.
    pub fn is_subset<S2>(&self, other: &IndexSet<T, S2>) -> bool
        where S2: BuildHasher
    {
        self.len() <= other.len() && self.iter().all(move |value| other.contains(value))
    }

    /// Returns `true` if all elements of `other` are contained in `self`.
    pub fn is_superset<S2>(&self, other: &IndexSet<T, S2>) -> bool
        where S2: BuildHasher
    {
        other.is_subset(self)
    }
}


/// A lazy iterator producing elements in the difference of `IndexSet`s.
///
/// This `struct` is created by the [`difference`] method on [`IndexSet`].
/// See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`difference`]: struct.IndexSet.html#method.difference
pub struct Difference<'a, T: 'a, S: 'a> {
    iter: Iter<'a, T>,
    other: &'a IndexSet<T, S>,
}

impl<'a, T, S> Iterator for Difference<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.iter.next() {
            if !self.other.contains(item) {
                return Some(item);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.iter.size_hint().1)
    }
}

impl<'a, T, S> DoubleEndedIterator for Difference<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.iter.next_back() {
            if !self.other.contains(item) {
                return Some(item);
            }
        }
        None
    }
}

impl<'a, T, S> Clone for Difference<'a, T, S> {
    fn clone(&self) -> Self {
        Difference { iter: self.iter.clone(), ..*self }
    }
}

impl<'a, T, S> fmt::Debug for Difference<'a, T, S>
    where T: fmt::Debug + Eq + Hash,
          S: BuildHasher
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}


/// A lazy iterator producing elements in the intersection of `IndexSet`s.
///
/// This `struct` is created by the [`intersection`] method on [`IndexSet`].
/// See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`intersection`]: struct.IndexSet.html#method.intersection
pub struct Intersection<'a, T: 'a, S: 'a> {
    iter: Iter<'a, T>,
    other: &'a IndexSet<T, S>,
}

impl<'a, T, S> Iterator for Intersection<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.iter.next() {
            if self.other.contains(item) {
                return Some(item);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.iter.size_hint().1)
    }
}

impl<'a, T, S> DoubleEndedIterator for Intersection<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher
{
    fn next_back(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.iter.next_back() {
            if self.other.contains(item) {
                return Some(item);
            }
        }
        None
    }
}

impl<'a, T, S> Clone for Intersection<'a, T, S> {
    fn clone(&self) -> Self {
        Intersection { iter: self.iter.clone(), ..*self }
    }
}

impl<'a, T, S> fmt::Debug for Intersection<'a, T, S>
    where T: fmt::Debug + Eq + Hash,
          S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}


/// A lazy iterator producing elements in the symmetric difference of `IndexSet`s.
///
/// This `struct` is created by the [`symmetric_difference`] method on
/// [`IndexSet`]. See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`symmetric_difference`]: struct.IndexSet.html#method.symmetric_difference
pub struct SymmetricDifference<'a, T: 'a, S1: 'a, S2: 'a> {
    iter: Chain<Difference<'a, T, S2>, Difference<'a, T, S1>>,
}

impl<'a, T, S1, S2> Iterator for SymmetricDifference<'a, T, S1, S2>
    where T: Eq + Hash,
          S1: BuildHasher,
          S2: BuildHasher,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn fold<B, F>(self, init: B, f: F) -> B
        where F: FnMut(B, Self::Item) -> B
    {
        self.iter.fold(init, f)
    }
}

impl<'a, T, S1, S2> DoubleEndedIterator for SymmetricDifference<'a, T, S1, S2>
    where T: Eq + Hash,
          S1: BuildHasher,
          S2: BuildHasher,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl<'a, T, S1, S2> Clone for SymmetricDifference<'a, T, S1, S2> {
    fn clone(&self) -> Self {
        SymmetricDifference { iter: self.iter.clone() }
    }
}

impl<'a, T, S1, S2> fmt::Debug for SymmetricDifference<'a, T, S1, S2>
    where T: fmt::Debug + Eq + Hash,
          S1: BuildHasher,
          S2: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}


/// A lazy iterator producing elements in the union of `IndexSet`s.
///
/// This `struct` is created by the [`union`] method on [`IndexSet`].
/// See its documentation for more.
///
/// [`IndexSet`]: struct.IndexSet.html
/// [`union`]: struct.IndexSet.html#method.union
pub struct Union<'a, T: 'a, S: 'a> {
    iter: Chain<Iter<'a, T>, Difference<'a, T, S>>,
}

impl<'a, T, S> Iterator for Union<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn fold<B, F>(self, init: B, f: F) -> B
        where F: FnMut(B, Self::Item) -> B
    {
        self.iter.fold(init, f)
    }
}

impl<'a, T, S> DoubleEndedIterator for Union<'a, T, S>
    where T: Eq + Hash,
          S: BuildHasher,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl<'a, T, S> Clone for Union<'a, T, S> {
    fn clone(&self) -> Self {
        Union { iter: self.iter.clone() }
    }
}

impl<'a, T, S> fmt::Debug for Union<'a, T, S>
    where T: fmt::Debug + Eq + Hash,
          S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}


impl<'a, 'b, T, S1, S2> BitAnd<&'b IndexSet<T, S2>> for &'a IndexSet<T, S1>
    where T: Eq + Hash + Clone,
          S1: BuildHasher + Default,
          S2: BuildHasher,
{
    type Output = IndexSet<T, S1>;

    /// Returns the set intersection, cloned into a new set.
    ///
    /// Values are collected in the same order that they appear in `self`.
    fn bitand(self, other: &'b IndexSet<T, S2>) -> Self::Output {
        self.intersection(other).cloned().collect()
    }
}

impl<'a, 'b, T, S1, S2> BitOr<&'b IndexSet<T, S2>> for &'a IndexSet<T, S1>
    where T: Eq + Hash + Clone,
          S1: BuildHasher + Default,
          S2: BuildHasher,
{
    type Output = IndexSet<T, S1>;

    /// Returns the set union, cloned into a new set.
    ///
    /// Values from `self` are collected in their original order, followed by
    /// values that are unique to `other` in their original order.
    fn bitor(self, other: &'b IndexSet<T, S2>) -> Self::Output {
        self.union(other).cloned().collect()
    }
}

impl<'a, 'b, T, S1, S2> BitXor<&'b IndexSet<T, S2>> for &'a IndexSet<T, S1>
    where T: Eq + Hash + Clone,
          S1: BuildHasher + Default,
          S2: BuildHasher,
{
    type Output = IndexSet<T, S1>;

    /// Returns the set symmetric-difference, cloned into a new set.
    ///
    /// Values from `self` are collected in their original order, followed by
    /// values from `other` in their original order.
    fn bitxor(self, other: &'b IndexSet<T, S2>) -> Self::Output {
        self.symmetric_difference(other).cloned().collect()
    }
}

impl<'a, 'b, T, S1, S2> Sub<&'b IndexSet<T, S2>> for &'a IndexSet<T, S1>
    where T: Eq + Hash + Clone,
          S1: BuildHasher + Default,
          S2: BuildHasher,
{
    type Output = IndexSet<T, S1>;

    /// Returns the set difference, cloned into a new set.
    ///
    /// Values are collected in the same order that they appear in `self`.
    fn sub(self, other: &'b IndexSet<T, S2>) -> Self::Output {
        self.difference(other).cloned().collect()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use util::enumerate;

    #[test]
    fn it_works() {
        let mut set = IndexSet::new();
        assert_eq!(set.is_empty(), true);
        set.insert(1);
        set.insert(1);
        assert_eq!(set.len(), 1);
        assert!(set.get(&1).is_some());
        assert_eq!(set.is_empty(), false);
    }

    #[test]
    fn new() {
        let set = IndexSet::<String>::new();
        println!("{:?}", set);
        assert_eq!(set.capacity(), 0);
        assert_eq!(set.len(), 0);
        assert_eq!(set.is_empty(), true);
    }

    #[test]
    fn insert() {
        let insert = [0, 4, 2, 12, 8, 7, 11, 5];
        let not_present = [1, 3, 6, 9, 10];
        let mut set = IndexSet::with_capacity(insert.len());

        for (i, &elt) in enumerate(&insert) {
            assert_eq!(set.len(), i);
            set.insert(elt);
            assert_eq!(set.len(), i + 1);
            assert_eq!(set.get(&elt), Some(&elt));
        }
        println!("{:?}", set);

        for &elt in &not_present {
            assert!(set.get(&elt).is_none());
        }
    }

    #[test]
    fn insert_full() {
        let insert = vec![9, 2, 7, 1, 4, 6, 13];
        let present = vec![1, 6, 2];
        let mut set = IndexSet::with_capacity(insert.len());

        for (i, &elt) in enumerate(&insert) {
            assert_eq!(set.len(), i);
            let (index, success) = set.insert_full(elt);
            assert!(success);
            assert_eq!(Some(index), set.get_full(&elt).map(|x| x.0));
            assert_eq!(set.len(), i + 1);
        }

        let len = set.len();
        for &elt in &present {
            let (index, success) = set.insert_full(elt);
            assert!(!success);
            assert_eq!(Some(index), set.get_full(&elt).map(|x| x.0));
            assert_eq!(set.len(), len);
        }
    }

    #[test]
    fn insert_2() {
        let mut set = IndexSet::with_capacity(16);

        let mut values = vec![];
        values.extend(0..16);
        values.extend(128..267);

        for &i in &values {
            let old_set = set.clone();
            set.insert(i);
            for value in old_set.iter() {
                if !set.get(value).is_some() {
                    println!("old_set: {:?}", old_set);
                    println!("set: {:?}", set);
                    panic!("did not find {} in set", value);
                }
            }
        }

        for &i in &values {
            assert!(set.get(&i).is_some(), "did not find {}", i);
        }
    }

    #[test]
    fn insert_dup() {
        let mut elements = vec![0, 2, 4, 6, 8];
        let mut set: IndexSet<u8> = elements.drain(..).collect();
        {
            let (i, v) = set.get_full(&0).unwrap();
            assert_eq!(set.len(), 5);
            assert_eq!(i, 0);
            assert_eq!(*v, 0);
        }
        {
            let inserted = set.insert(0);
            let (i, v) = set.get_full(&0).unwrap();
            assert_eq!(set.len(), 5);
            assert_eq!(inserted, false);
            assert_eq!(i, 0);
            assert_eq!(*v, 0);
        }
    }

    #[test]
    fn insert_order() {
        let insert = [0, 4, 2, 12, 8, 7, 11, 5, 3, 17, 19, 22, 23];
        let mut set = IndexSet::new();

        for &elt in &insert {
            set.insert(elt);
        }

        assert_eq!(set.iter().count(), set.len());
        assert_eq!(set.iter().count(), insert.len());
        for (a, b) in insert.iter().zip(set.iter()) {
            assert_eq!(a, b);
        }
        for (i, v) in (0..insert.len()).zip(set.iter()) {
            assert_eq!(set.get_index(i).unwrap(), v);
        }
    }

    #[test]
    fn grow() {
        let insert = [0, 4, 2, 12, 8, 7, 11];
        let not_present = [1, 3, 6, 9, 10];
        let mut set = IndexSet::with_capacity(insert.len());


        for (i, &elt) in enumerate(&insert) {
            assert_eq!(set.len(), i);
            set.insert(elt);
            assert_eq!(set.len(), i + 1);
            assert_eq!(set.get(&elt), Some(&elt));
        }

        println!("{:?}", set);
        for &elt in &insert {
            set.insert(elt * 10);
        }
        for &elt in &insert {
            set.insert(elt * 100);
        }
        for (i, &elt) in insert.iter().cycle().enumerate().take(100) {
            set.insert(elt * 100 + i as i32);
        }
        println!("{:?}", set);
        for &elt in &not_present {
            assert!(set.get(&elt).is_none());
        }
    }

    #[test]
    fn remove() {
        let insert = [0, 4, 2, 12, 8, 7, 11, 5, 3, 17, 19, 22, 23];
        let mut set = IndexSet::new();

        for &elt in &insert {
            set.insert(elt);
        }

        assert_eq!(set.iter().count(), set.len());
        assert_eq!(set.iter().count(), insert.len());
        for (a, b) in insert.iter().zip(set.iter()) {
            assert_eq!(a, b);
        }

        let remove_fail = [99, 77];
        let remove = [4, 12, 8, 7];

        for &value in &remove_fail {
            assert!(set.swap_remove_full(&value).is_none());
        }
        println!("{:?}", set);
        for &value in &remove {
        //println!("{:?}", set);
            let index = set.get_full(&value).unwrap().0;
            assert_eq!(set.swap_remove_full(&value), Some((index, value)));
        }
        println!("{:?}", set);

        for value in &insert {
            assert_eq!(set.get(value).is_some(), !remove.contains(value));
        }
        assert_eq!(set.len(), insert.len() - remove.len());
        assert_eq!(set.iter().count(), insert.len() - remove.len());
    }

    #[test]
    fn swap_remove_index() {
        let insert = [0, 4, 2, 12, 8, 7, 11, 5, 3, 17, 19, 22, 23];
        let mut set = IndexSet::new();

        for &elt in &insert {
            set.insert(elt);
        }

        let mut vector = insert.to_vec();
        let remove_sequence = &[3, 3, 10, 4, 5, 4, 3, 0, 1];

        // check that the same swap remove sequence on vec and set
        // have the same result.
        for &rm in remove_sequence {
            let out_vec = vector.swap_remove(rm);
            let out_set = set.swap_remove_index(rm).unwrap();
            assert_eq!(out_vec, out_set);
        }
        assert_eq!(vector.len(), set.len());
        for (a, b) in vector.iter().zip(set.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn partial_eq_and_eq() {
        let mut set_a = IndexSet::new();
        set_a.insert(1);
        set_a.insert(2);
        let mut set_b = set_a.clone();
        assert_eq!(set_a, set_b);
        set_b.swap_remove(&1);
        assert_ne!(set_a, set_b);

        let set_c: IndexSet<_> = set_b.into_iter().collect();
        assert_ne!(set_a, set_c);
        assert_ne!(set_c, set_a);
    }

    #[test]
    fn extend() {
        let mut set = IndexSet::new();
        set.extend(vec![&1, &2, &3, &4]);
        set.extend(vec![5, 6]);
        assert_eq!(set.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn comparisons() {
        let set_a: IndexSet<_> = (0..3).collect();
        let set_b: IndexSet<_> = (3..6).collect();
        let set_c: IndexSet<_> = (0..6).collect();
        let set_d: IndexSet<_> = (3..9).collect();

        assert!(!set_a.is_disjoint(&set_a));
        assert!(set_a.is_subset(&set_a));
        assert!(set_a.is_superset(&set_a));

        assert!(set_a.is_disjoint(&set_b));
        assert!(set_b.is_disjoint(&set_a));
        assert!(!set_a.is_subset(&set_b));
        assert!(!set_b.is_subset(&set_a));
        assert!(!set_a.is_superset(&set_b));
        assert!(!set_b.is_superset(&set_a));

        assert!(!set_a.is_disjoint(&set_c));
        assert!(!set_c.is_disjoint(&set_a));
        assert!(set_a.is_subset(&set_c));
        assert!(!set_c.is_subset(&set_a));
        assert!(!set_a.is_superset(&set_c));
        assert!(set_c.is_superset(&set_a));

        assert!(!set_c.is_disjoint(&set_d));
        assert!(!set_d.is_disjoint(&set_c));
        assert!(!set_c.is_subset(&set_d));
        assert!(!set_d.is_subset(&set_c));
        assert!(!set_c.is_superset(&set_d));
        assert!(!set_d.is_superset(&set_c));
    }

    #[test]
    fn iter_comparisons() {
        use std::iter::empty;

        fn check<'a, I1, I2>(iter1: I1, iter2: I2)
            where I1: Iterator<Item = &'a i32>,
                  I2: Iterator<Item = i32>,
        {
            assert!(iter1.cloned().eq(iter2));
        }

        let set_a: IndexSet<_> = (0..3).collect();
        let set_b: IndexSet<_> = (3..6).collect();
        let set_c: IndexSet<_> = (0..6).collect();
        let set_d: IndexSet<_> = (3..9).rev().collect();

        check(set_a.difference(&set_a), empty());
        check(set_a.symmetric_difference(&set_a), empty());
        check(set_a.intersection(&set_a), 0..3);
        check(set_a.union(&set_a), 0..3);

        check(set_a.difference(&set_b), 0..3);
        check(set_b.difference(&set_a), 3..6);
        check(set_a.symmetric_difference(&set_b), 0..6);
        check(set_b.symmetric_difference(&set_a), (3..6).chain(0..3));
        check(set_a.intersection(&set_b), empty());
        check(set_b.intersection(&set_a), empty());
        check(set_a.union(&set_b), 0..6);
        check(set_b.union(&set_a), (3..6).chain(0..3));

        check(set_a.difference(&set_c), empty());
        check(set_c.difference(&set_a), 3..6);
        check(set_a.symmetric_difference(&set_c), 3..6);
        check(set_c.symmetric_difference(&set_a), 3..6);
        check(set_a.intersection(&set_c), 0..3);
        check(set_c.intersection(&set_a), 0..3);
        check(set_a.union(&set_c), 0..6);
        check(set_c.union(&set_a), 0..6);

        check(set_c.difference(&set_d), 0..3);
        check(set_d.difference(&set_c), (6..9).rev());
        check(set_c.symmetric_difference(&set_d), (0..3).chain((6..9).rev()));
        check(set_d.symmetric_difference(&set_c), (6..9).rev().chain(0..3));
        check(set_c.intersection(&set_d), 3..6);
        check(set_d.intersection(&set_c), (3..6).rev());
        check(set_c.union(&set_d), (0..6).chain((6..9).rev()));
        check(set_d.union(&set_c), (3..9).rev().chain(0..3));
    }

    #[test]
    fn ops() {
        let empty = IndexSet::<i32>::new();
        let set_a: IndexSet<_> = (0..3).collect();
        let set_b: IndexSet<_> = (3..6).collect();
        let set_c: IndexSet<_> = (0..6).collect();
        let set_d: IndexSet<_> = (3..9).rev().collect();

        assert_eq!(&set_a & &set_a, set_a);
        assert_eq!(&set_a | &set_a, set_a);
        assert_eq!(&set_a ^ &set_a, empty);
        assert_eq!(&set_a - &set_a, empty);

        assert_eq!(&set_a & &set_b, empty);
        assert_eq!(&set_b & &set_a, empty);
        assert_eq!(&set_a | &set_b, set_c);
        assert_eq!(&set_b | &set_a, set_c);
        assert_eq!(&set_a ^ &set_b, set_c);
        assert_eq!(&set_b ^ &set_a, set_c);
        assert_eq!(&set_a - &set_b, set_a);
        assert_eq!(&set_b - &set_a, set_b);

        assert_eq!(&set_a & &set_c, set_a);
        assert_eq!(&set_c & &set_a, set_a);
        assert_eq!(&set_a | &set_c, set_c);
        assert_eq!(&set_c | &set_a, set_c);
        assert_eq!(&set_a ^ &set_c, set_b);
        assert_eq!(&set_c ^ &set_a, set_b);
        assert_eq!(&set_a - &set_c, empty);
        assert_eq!(&set_c - &set_a, set_b);

        assert_eq!(&set_c & &set_d, set_b);
        assert_eq!(&set_d & &set_c, set_b);
        assert_eq!(&set_c | &set_d, &set_a | &set_d);
        assert_eq!(&set_d | &set_c, &set_a | &set_d);
        assert_eq!(&set_c ^ &set_d, &set_a | &(&set_d - &set_b));
        assert_eq!(&set_d ^ &set_c, &set_a | &(&set_d - &set_b));
        assert_eq!(&set_c - &set_d, set_a);
        assert_eq!(&set_d - &set_c, &set_d - &set_b);
    }
}
