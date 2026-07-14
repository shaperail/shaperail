ALTER TABLE projects
  ALTER COLUMN created_by TYPE TEXT USING created_by::text;

ALTER TABLE tasks
  ALTER COLUMN assigned_to TYPE TEXT USING assigned_to::text,
  ALTER COLUMN created_by TYPE TEXT USING created_by::text;

ALTER TABLE tasks DROP CONSTRAINT tasks_status_check;
ALTER TABLE tasks
  ADD CONSTRAINT tasks_status_check
  CHECK (status IN ('todo', 'in_progress', 'done', 'archived'));
