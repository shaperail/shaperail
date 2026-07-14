ALTER TABLE customers
  ALTER COLUMN created_by TYPE TEXT USING created_by::text;

ALTER TABLE invoices
  ALTER COLUMN created_by TYPE TEXT USING created_by::text;

ALTER TABLE payments
  ALTER COLUMN processed_by TYPE TEXT USING processed_by::text;
