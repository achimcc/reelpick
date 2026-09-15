//! Jellyfin: the library, the films in it, their posters.

#[derive(Debug, Clone, PartialEq)]
pub struct Movie {
    pub item_id: String,
    pub title: String,
    pub year: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub genres: Vec<String>,
    pub director: Option<String>,
    pub overview: Option<String>,
    pub runtime_min: Option<i64>,
    pub community_rating: Option<f64>,
}
