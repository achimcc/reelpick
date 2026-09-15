//! Choosing: pure functions over candidates. Filled in by task 4.

use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct Excluded {
    pub tmdb_ids: HashSet<i64>,
    pub item_ids: HashSet<String>,
}
