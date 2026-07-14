UPDATE posts SET status = 'draft' WHERE status IS NULL;
UPDATE posts SET created_at = NOW() WHERE created_at IS NULL;
UPDATE posts SET updated_at = NOW() WHERE updated_at IS NULL;
UPDATE comments SET created_at = NOW() WHERE created_at IS NULL;
UPDATE comments SET updated_at = NOW() WHERE updated_at IS NULL;

ALTER TABLE posts
  ALTER COLUMN status SET NOT NULL,
  ALTER COLUMN created_by TYPE TEXT USING created_by::text,
  ALTER COLUMN created_at SET NOT NULL,
  ALTER COLUMN updated_at SET NOT NULL;

ALTER TABLE posts DROP CONSTRAINT chk_posts_status;
ALTER TABLE posts
  ADD CONSTRAINT chk_posts_status
  CHECK (status IN ('draft', 'published', 'archived'));

ALTER TABLE comments
  ALTER COLUMN created_by TYPE TEXT USING created_by::text,
  ALTER COLUMN created_at SET NOT NULL,
  ALTER COLUMN updated_at SET NOT NULL;
