CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "posts" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "title" VARCHAR(200) NOT NULL,
  "slug" VARCHAR(200) NOT NULL UNIQUE,
  "body" TEXT NOT NULL,
  "status" TEXT DEFAULT 'draft',
  "created_by" UUID NOT NULL,
  "published_at" TIMESTAMPTZ,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  "deleted_at" TIMESTAMPTZ,
  CONSTRAINT "chk_posts_status" CHECK ("status" IN ('draft', 'published'))
);

CREATE UNIQUE INDEX IF NOT EXISTS "idx_posts_0" ON "posts" ("slug");
CREATE INDEX IF NOT EXISTS "idx_posts_1" ON "posts" ("created_at" DESC);
