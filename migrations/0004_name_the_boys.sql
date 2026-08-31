-- 0004_name_the_boys — the owner names the four seeded profiles (T1.4 follow-up).
--
-- `0003_profiles.sql` seeded four placeholder identities, `'Boy 1'..'Boy 4'`,
-- at ids 1..4. The owner has now chosen real names. PURPLE §R-13's standing
-- rule is **never edit an applied migration**, so this lands as a new
-- numbered migration rather than a change to 0003.
--
-- Each UPDATE matches on the *exact* seeded placeholder name, not on `id`, so
-- it is idempotent and non-destructive: a family that already renamed a
-- profile on the phone (e.g. id 2 from "Boy 2" to "Nate") keeps that name —
-- the row's current name no longer matches `'Boy 2'`, so this migration
-- leaves it untouched. Re-running this migration (or running it against a
-- fresh database seeded straight from 0003) is a no-op the second time, since
-- by then no row's name is still `'Boy N'`.

UPDATE profiles SET name = 'Isaiah'    WHERE name = 'Boy 1';
UPDATE profiles SET name = 'Nathaniel' WHERE name = 'Boy 2';
UPDATE profiles SET name = 'Simeon'    WHERE name = 'Boy 3';
UPDATE profiles SET name = 'Ezekiel'   WHERE name = 'Boy 4';
