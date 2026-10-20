import type { ConsoleBundle } from "bmo-ui/shared/console-bundle";

// What a console bundle declares, as the console reads it (bmo-ui's `docs/console-bundles.md`): the
// screen's path, name and icon, the service its screens call, whether its stream and its sockets are
// relayed, and the event kinds a notice is raised for. The declaration is the bundle's; the console
// attaches it by this alone (`modules/console-bundles.ts`).
export type ConsoleBundleView = ConsoleBundle;
