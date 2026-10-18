import type { GatePartView } from "../../apps/web/shared/types";

// A gate order as the gate unit serves one: two parts of a turn, the second with two gates starting together,
// one gate standing in both parts, two conditions, and a gate outside any turn. The gate unit is not split, so
// the console's tests hold an order of their own with neutral names.
export const GATE_ORDER: GatePartView[] = [
  {
    part: "pre",
    inTurn: true,
    steps: [
      { places: [{ key: "gate-a", condition: null }] },
      { places: [{ key: "gate-b", condition: null }, { key: "gate-c", condition: "fresh-session" }] }
    ]
  },
  {
    part: "post",
    inTurn: true,
    steps: [{ places: [{ key: "gate-a", condition: null }] }, { places: [{ key: "gate-d", condition: "folding-dialect" }] }]
  },
  { part: "apart", inTurn: false, steps: [{ places: [{ key: "gate-e", condition: null }] }] }
];
