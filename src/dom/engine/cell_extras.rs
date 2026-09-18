//! Specialized methods for `Cell` of some specific `!Copy` types, allowing
//! limited access to a value without moving it out of the cell.
//!
//! Vendored from `kuchikiki` (see the attribution notice in
//! `crate::dom::engine`), unchanged.
//!
//! # Soundness
//!
//! These methods use `Cell::as_ptr` and `unsafe`. Their soundness lies in
//! that:
//!
//! * `Cell<T>: !Sync` for any `T`, so no other thread is accessing this
//!   cell.
//! * For the duration of the raw pointer access, this thread only runs
//!   code that is known to not access the same cell again. In particular,
//!   no method of a type parameter is called.

use std::cell::Cell;
use std::rc::{Rc, Weak};

pub trait CellOption {
    fn is_none(&self) -> bool;
}

impl<T> CellOption for Cell<Option<T>> {
    #[inline]
    fn is_none(&self) -> bool {
        unsafe { (*self.as_ptr()).is_none() }
    }
}

pub trait CellOptionWeak<T> {
    fn upgrade(&self) -> Option<Rc<T>>;
    fn clone_inner(&self) -> Option<Weak<T>>;
}

impl<T> CellOptionWeak<T> for Cell<Option<Weak<T>>> {
    #[inline]
    fn upgrade(&self) -> Option<Rc<T>> {
        unsafe { (*self.as_ptr()).as_ref().and_then(Weak::upgrade) }
    }

    #[inline]
    fn clone_inner(&self) -> Option<Weak<T>> {
        unsafe { (*self.as_ptr()).clone() }
    }
}

pub trait CellOptionRc<T> {
    /// Return `Some` if this `Rc` is the only strong reference count, even
    /// if there are weak references.
    fn take_if_unique_strong(&self) -> Option<Rc<T>>;
    fn clone_inner(&self) -> Option<Rc<T>>;
}

impl<T> CellOptionRc<T> for Cell<Option<Rc<T>>> {
    #[inline]
    fn take_if_unique_strong(&self) -> Option<Rc<T>> {
        unsafe {
            match *self.as_ptr() {
                None => None,
                Some(ref rc) if Rc::strong_count(rc) > 1 => None,
                // Not borrowing the `Rc<T>` here as we would be
                // invalidating that borrow while it is outstanding:
                Some(_) => self.take(),
            }
        }
    }

    #[inline]
    fn clone_inner(&self) -> Option<Rc<T>> {
        unsafe { (*self.as_ptr()).clone() }
    }
}
