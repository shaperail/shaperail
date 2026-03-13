CREATE TABLE IF NOT EXISTS "test_orgs" (
  "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  "name" TEXT NOT NULL
);
