ALTER TABLE fin_report_schedules ADD COLUMN anchor_day smallint;
UPDATE fin_report_schedules SET anchor_day = EXTRACT(day FROM next_run_date);
ALTER TABLE fin_report_schedules ALTER COLUMN anchor_day SET NOT NULL;
ALTER TABLE fin_report_schedules ADD CONSTRAINT fin_report_schedules_anchor_day_check CHECK (anchor_day BETWEEN 1 AND 31);
