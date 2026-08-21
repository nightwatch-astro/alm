-- Drop the unused `require_same_camera` flag from the calibration tolerances
-- singleton. The matching engine has no camera dimension to relax, so nothing
-- ever read the column.
ALTER TABLE calibration_tolerances DROP COLUMN require_same_camera;
