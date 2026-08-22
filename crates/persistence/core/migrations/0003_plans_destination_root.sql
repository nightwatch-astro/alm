-- Absolute destination root a plan's items must resolve under.
--
-- Source-view generation stores an absolute `plan_items.to_relative_path` with
-- `to_root_id` NULL, so the executor had no root to contain the destination
-- against and fell back to the item's SOURCE root. NULL keeps the destination
-- gate inactive for plans that predate the column.
ALTER TABLE "plans" ADD COLUMN destination_root TEXT;
