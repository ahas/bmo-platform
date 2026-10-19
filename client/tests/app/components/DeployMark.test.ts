import { describe, expect, test } from "bun:test";

// The deployment popover's open state is shared by every mark, and a screen the router keeps set
// aside keeps its mark: only the mark of the screen standing opens, so a kept screen's popover never
// stands at the window's corner beside the real one (docs/web.md, 「Deployment in the title bar」).

const mark = await Bun.file(new URL("../../../apps/web/app/components/DeployMark.vue", import.meta.url)).text();

describe("the deployment popover", () => {
  test("opens on the standing screen's mark alone", () => {
    expect(mark).toContain('v-model:open="shown"');
    expect(mark).toContain("get: () => open.value && standing.value");
    expect(mark).toContain("onDeactivated(() => (standing.value = false));");
    expect(mark).toContain("onActivated(() => (standing.value = true));");
  });

  test("reads the run's log only while it is shown", () => {
    expect(mark).toContain("() => [shown.value, view.value?.run?.phase ?? null");
  });
});
