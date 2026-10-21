//! Hardware adapter implementations, enabled by their respective features.

#[cfg(all(target_os = "windows", feature = "j2534"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "j2534"))))]
pub mod j2534;

#[cfg(feature = "panda")]
#[cfg_attr(docsrs, doc(cfg(feature = "panda")))]
pub mod panda;

#[cfg(feature = "peak")]
#[cfg_attr(docsrs, doc(cfg(feature = "peak")))]
pub mod peak;

#[cfg(all(target_os = "linux", feature = "socketcan"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "linux", feature = "socketcan"))))]
pub mod socketcan;

#[cfg(all(target_os = "windows", feature = "vector-xl"))]
#[cfg_attr(docsrs, doc(cfg(all(target_os = "windows", feature = "vector-xl"))))]
pub mod vector;
