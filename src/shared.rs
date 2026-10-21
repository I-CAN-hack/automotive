//! Borrowed or reference-counted ownership for protocol resources.

use std::{ops::Deref, sync::Arc};

pub(crate) enum Shared<'a, T> {
    Borrowed(&'a T),
    Owned(Arc<T>),
}

impl<T> Clone for Shared<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Borrowed(value) => Self::Borrowed(value),
            Self::Owned(value) => Self::Owned(value.clone()),
        }
    }
}

impl<T> Deref for Shared<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Self::Borrowed(value) => value,
            Self::Owned(value) => value,
        }
    }
}
