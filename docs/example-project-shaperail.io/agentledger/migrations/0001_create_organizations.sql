CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS "organizations" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "name" VARCHAR(200) NOT NULL,
  "plan" TEXT DEFAULT 'free',
  "base_currency" VARCHAR(3) NOT NULL,
  "gl_provider" TEXT DEFAULT 'none',
  "created_at" TIMESTAMPTZ DEFAULT NOW(),
  "updated_at" TIMESTAMPTZ DEFAULT NOW(),
  "deleted_at" TIMESTAMPTZ,
  CONSTRAINT "chk_organizations_plan" CHECK ("plan" IN ('free', 'growth', 'enterprise')),
  CONSTRAINT "chk_organizations_gl_provider" CHECK ("gl_provider" IN ('none', 'quickbooks_online', 'netsuite', 'csv'))
);
CREATE INDEX IF NOT EXISTS "idx_organizations_0" ON "organizations" ("name");
