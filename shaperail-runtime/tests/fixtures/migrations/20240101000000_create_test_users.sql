CREATE TABLE IF NOT EXISTS "test_users" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "email" VARCHAR(255) NOT NULL UNIQUE,
  "name" VARCHAR(200) NOT NULL,
  "role" TEXT NOT NULL DEFAULT 'member',
  "org_id" UUID NOT NULL,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  "deleted_at" TIMESTAMPTZ,
  CONSTRAINT "chk_test_users_role" CHECK ("role" IN ('admin', 'member', 'viewer'))
);

CREATE INDEX IF NOT EXISTS "idx_test_users_org_role" ON "test_users" ("org_id", "role");
CREATE INDEX IF NOT EXISTS "idx_test_users_created" ON "test_users" ("created_at" DESC);
