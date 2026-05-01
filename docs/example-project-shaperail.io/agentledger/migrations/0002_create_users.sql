CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "users" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "org_id" UUID NOT NULL,
  "email" TEXT NOT NULL UNIQUE,
  "name" VARCHAR(200) NOT NULL,
  "role" TEXT DEFAULT 'viewer',
  "password_hash" VARCHAR(255) NOT NULL,
  "deleted_at" TIMESTAMPTZ,
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  CONSTRAINT "fk_users_org_id" FOREIGN KEY ("org_id") REFERENCES "organizations"("id"),
  CONSTRAINT "chk_users_role" CHECK ("role" IN ('super_admin', 'admin', 'finance', 'viewer'))
);
CREATE INDEX IF NOT EXISTS "idx_users_0" ON "users" ("org_id", "role");
CREATE UNIQUE INDEX IF NOT EXISTS "idx_users_1" ON "users" ("email");
