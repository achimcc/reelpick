-- The IMDb rating of the picked film, as Jellyfin holds it (the IMDb Ratings
-- plugin writes IMDb's own number into CommunityRating). NULL for every pick
-- made before this column existed, and for a film IMDb has no rating for.
ALTER TABLE picks ADD COLUMN imdb_rating REAL;
