// The keys of the registered models' addresses (docs/own-llm.md, 「The keys」): written, taken away and
// checked by the harness-machine binary (docs/harness-machine.md, 「The keys」), which reads a key off
// its standing input alone. Nothing here returns a key or writes one anywhere else.

// What a screen is shown of an address's key: usable, none named, or named but not usable.
export type KeyState = "usable" | "none" | "unusable";

