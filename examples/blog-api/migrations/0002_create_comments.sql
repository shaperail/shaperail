CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "comments" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "post_id" UUID NOT NULL,
  "body" TEXT NOT NULL,
  "author_name" VARCHAR(100) NOT NULL,
  "created_by" UUID NOT NULL,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  CONSTRAINT "fk_comments_post_id" FOREIGN KEY ("post_id") REFERENCES "posts"("id")
);

CREATE INDEX IF NOT EXISTS "idx_comments_0" ON "comments" ("post_id");
CREATE INDEX IF NOT EXISTS "idx_comments_1" ON "comments" ("created_at" DESC);
