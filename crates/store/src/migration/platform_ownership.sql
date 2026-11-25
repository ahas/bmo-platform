-- Ownership of a row stands in two columns everywhere: "owner_account" for a person's row, "owner_unit" for an
-- organisation unit's, neither for the platform's, and never both. The tables that named an owner otherwise
-- take the names, and a seat's account stands as a number. The unit a run's ledger row names its work by is
-- the "work_unit", leaving the two ownership columns free.

ALTER TABLE "supervisors" ADD COLUMN "owner_unit" bigint;
ALTER TABLE "supervisors" ADD CONSTRAINT "supervisors_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "role_agents" ADD COLUMN "owner_unit" bigint;
ALTER TABLE "role_agents" ALTER COLUMN "owner_account" TYPE bigint
  USING NULLIF(btrim("owner_account"), '')::bigint;
ALTER TABLE "role_agents" ADD CONSTRAINT "role_agents_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "registered_models" RENAME COLUMN "account" TO "owner_account";
ALTER TABLE "registered_models" ADD COLUMN "owner_unit" bigint;
ALTER TABLE "registered_models" ADD CONSTRAINT "registered_models_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "claude_accounts" RENAME COLUMN "user_id" TO "owner_account";
ALTER TABLE "claude_accounts" RENAME COLUMN "unit_id" TO "owner_unit";
ALTER TABLE "claude_accounts" ADD CONSTRAINT "claude_accounts_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "personal_supply" RENAME COLUMN "user_id" TO "owner_account";
ALTER TABLE "personal_supply" RENAME COLUMN "unit_id" TO "owner_unit";
ALTER TABLE "personal_supply" DROP CONSTRAINT "personal_supply_one_owner";
ALTER TABLE "personal_supply" ADD CONSTRAINT "personal_supply_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "repositories" ADD CONSTRAINT "repositories_one_owner"
  CHECK (NOT ("owner_account" IS NOT NULL AND "owner_unit" IS NOT NULL));

ALTER TABLE "runs" RENAME COLUMN "unit" TO "work_unit";
ALTER TABLE "runs" ADD COLUMN "owner_account" bigint;
ALTER TABLE "runs" ADD COLUMN "owner_unit" bigint;
