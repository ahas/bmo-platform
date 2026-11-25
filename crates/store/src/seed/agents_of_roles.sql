INSERT INTO "role_agents"
  ("name", "role", "provider", "model", "effort", "thinking", "answer_tokens", "enabled", "added_at")
SELECT r."name" || '-' || lpad(n::text, 2, '0'),
       r."name",
       COALESCE(r."active_provider", picked.entry ->> 'provider'),
       COALESCE(r."active_model", picked.entry ->> 'model'),
       picked.entry ->> 'effort',
       picked.entry ->> 'thinking',
       (picked.entry ->> 'answerTokens')::bigint,
       1,
       r."created_at"
FROM "roles" r
CROSS JOIN LATERAL generate_series(1, GREATEST(r."max_concurrent", 0)) AS n
LEFT JOIN LATERAL (
  SELECT listed.entry FROM jsonb_array_elements(r."provider_order"::jsonb) WITH ORDINALITY AS listed(entry, place)
  WHERE (r."active_provider" IS NULL AND NOT EXISTS (
           SELECT 1 FROM (VALUES
             ('codex', 'search'), ('codex', 'browse'), ('codex', 'translate'),
             ('openrouter', 'search'), ('openrouter', 'browse'), ('openrouter', 'translate'),
             ('deepseek', 'browse'), ('deepseek', 'translate'),
             ('registered', 'browse')
           ) AS uncarried("provider", "line")
           WHERE uncarried."provider" = listed.entry ->> 'provider' AND uncarried."line" = r."line"
         ))
     OR listed.entry ->> 'provider' = r."active_provider"
  ORDER BY listed.place
  LIMIT 1
) AS picked ON true
WHERE NOT EXISTS (SELECT 1 FROM "role_agents" a WHERE a."role" = r."name")
