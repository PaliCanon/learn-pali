//! The service's one page, the home: an outer page (no props: it reads
//! the store in an Effect after hydration) and inner views (what it shows
//! for a given state, rendered by the tests with an injected state, the
//! embedded deck and a fixed `today`). The review runs in place on it.

pub(crate) mod home;
