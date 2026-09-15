//! Choosing: pure functions over candidates, so the rules can be tested
//! without a single network call.

use std::collections::HashSet;

use rand::seq::SliceRandom;

use crate::jellyfin::Movie;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub movie: Movie,
    pub rating: Option<f64>,
    pub votes: Option<i64>,
    pub overview: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub min_rating: f64,
    pub min_votes: i64,
}

#[derive(Debug, Default, Clone)]
pub struct Excluded {
    pub tmdb_ids: HashSet<i64>,
    pub item_ids: HashSet<String>,
}

/// What may be picked today: identified, not yet picked, and rated well
/// enough by enough people. A candidate without a vote count (the Jellyfin
/// fallback) is judged on the rating alone.
pub fn eligible(candidates: Vec<Candidate>, t: &Thresholds, ex: &Excluded) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|c| {
            let Some(tmdb_id) = c.movie.tmdb_id else {
                return false;
            };
            if ex.tmdb_ids.contains(&tmdb_id) || ex.item_ids.contains(&c.movie.item_id) {
                return false;
            }
            let Some(rating) = c.rating else { return false };
            if rating < t.min_rating {
                return false;
            }
            match c.votes {
                Some(v) => v >= t.min_votes,
                None => true,
            }
        })
        .collect()
}

/// `n` candidates, uniformly at random. The threshold has already sorted;
/// a 6.6 gets the same chance as an 8.4, or a year of picks is all classics.
pub fn sample<R: rand::Rng>(rng: &mut R, mut pool: Vec<Candidate>, n: usize) -> Vec<Candidate> {
    pool.shuffle(rng);
    pool.truncate(n);
    pool
}

/// The fallback when no model answers: the best rating, ties broken by votes.
pub fn best_rated(pool: &[Candidate]) -> Option<&Candidate> {
    pool.iter().max_by(|a, b| {
        let ra = a.rating.unwrap_or(f64::MIN);
        let rb = b.rating.unwrap_or(f64::MIN);
        ra.total_cmp(&rb)
            .then(a.votes.unwrap_or(0).cmp(&b.votes.unwrap_or(0)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jellyfin::Movie;
    use rand::SeedableRng;

    fn movie(id: i64, item: &str) -> Movie {
        Movie {
            item_id: item.into(),
            title: format!("Film {id}"),
            year: Some(2000),
            tmdb_id: Some(id),
            genres: vec![],
            director: None,
            overview: None,
            runtime_min: None,
            community_rating: None,
        }
    }
    fn cand(id: i64, rating: Option<f64>, votes: Option<i64>) -> Candidate {
        Candidate {
            movie: movie(id, &format!("item-{id}")),
            rating,
            votes,
            overview: String::new(),
        }
    }
    fn t() -> Thresholds {
        Thresholds {
            min_rating: 6.5,
            min_votes: 200,
        }
    }

    #[test]
    fn the_threshold_keeps_what_is_rated_and_voted_enough() {
        let pool = vec![
            cand(1, Some(7.0), Some(500)),
            cand(2, Some(6.4), Some(500)),
            cand(3, Some(9.0), Some(12)),
        ];
        let ids: Vec<_> = eligible(pool, &t(), &Excluded::default())
            .iter()
            .map(|c| c.movie.tmdb_id.unwrap())
            .collect();
        assert_eq!(ids, [1]);
    }

    #[test]
    fn without_votes_only_the_rating_counts_and_without_rating_nothing_does() {
        let pool = vec![cand(1, Some(7.0), None), cand(2, None, None)];
        let ids: Vec<_> = eligible(pool, &t(), &Excluded::default())
            .iter()
            .map(|c| c.movie.tmdb_id.unwrap())
            .collect();
        assert_eq!(ids, [1]);
    }

    #[test]
    fn excluded_by_either_id_and_missing_tmdb_id_fall_out() {
        let mut nameless = cand(4, Some(8.0), Some(900));
        nameless.movie.tmdb_id = None;
        let pool = vec![
            cand(1, Some(8.0), Some(900)),
            cand(2, Some(8.0), Some(900)),
            cand(3, Some(8.0), Some(900)),
            nameless,
        ];
        let mut ex = Excluded::default();
        ex.tmdb_ids.insert(1);
        ex.item_ids.insert("item-2".into());
        let ids: Vec<_> = eligible(pool, &t(), &ex)
            .iter()
            .map(|c| c.movie.tmdb_id.unwrap())
            .collect();
        assert_eq!(ids, [3]);
    }

    #[test]
    fn sample_is_reproducible_with_a_seed_and_never_larger_than_the_pool() {
        let pool: Vec<_> = (1..=20).map(|i| cand(i, Some(7.0), Some(500))).collect();
        let mut a = rand::rngs::StdRng::seed_from_u64(42);
        let mut b = rand::rngs::StdRng::seed_from_u64(42);
        let sa: Vec<_> = sample(&mut a, pool.clone(), 5)
            .iter()
            .map(|c| c.movie.tmdb_id)
            .collect();
        let sb: Vec<_> = sample(&mut b, pool.clone(), 5)
            .iter()
            .map(|c| c.movie.tmdb_id)
            .collect();
        assert_eq!(sa, sb);
        assert_eq!(sa.len(), 5);
        let small = sample(&mut a, pool[..3].to_vec(), 5);
        assert_eq!(small.len(), 3);
    }

    #[test]
    fn best_rated_prefers_the_rating_then_the_votes() {
        let pool = vec![
            cand(1, Some(7.0), Some(500)),
            cand(2, Some(8.1), Some(300)),
            cand(3, Some(8.1), Some(900)),
        ];
        assert_eq!(best_rated(&pool).unwrap().movie.tmdb_id, Some(3));
        assert!(best_rated(&[]).is_none());
    }
}
