-- The deleted legacy fork route let a user fork their own draft, so a
-- deployed DB may have forked_from edges pointing at draft rows. create()'s
-- stale-draft sweep would trip the forked_from FK on those rows, so detach
-- them here; new drafts can never be a fork target (create only accepts a
-- published forkedFrom).
UPDATE toys SET forked_from=NULL WHERE forked_from IN (SELECT id FROM toys WHERE state='draft');
