// Agency's own composition of its agent base, for a process agency itself starts — the loop in a
// pane, the prompt-composing entry, the pane guardian: the providers this build carries, the lines,
// the verb a place's status line and hooks call, and the work keys the policy declares. What only a
// consumer can say — a gate's road, a tool record's extras — the consumer registers beside these, and
// a process that imports none of it runs on the defaults.

import "./harnesses";

import { composeRunLines } from "./line-declarations";
import { agencyRunLines } from "./run-lines";
import { composeAgentVerbs } from "./verbs";
import { policyWork } from "./work-keys";
import { composeWorkProfiles } from "./work-profile";

composeRunLines(agencyRunLines);
// The verb the agent base starts by path: the one a place's status line and hooks call.
composeAgentVerbs({ statusLine: ["bin/bmo-agency", "agent-status"] });
composeWorkProfiles(policyWork);
