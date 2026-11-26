-- A slot request keeps the units of its account's path, as the delegation chain answered them when the ask was
-- made: the seats filed under them stand among the agents the request may take and the cap it is counted
-- against (docs/role-slots.md, "Accounts").

ALTER TABLE "role_slot_requests" ADD COLUMN "units" text DEFAULT '[]'::text NOT NULL;
