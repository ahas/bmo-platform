import { describe, expect, test } from "bun:test";

// The box the owner types into at the foot of an agent's own screen, and where what he types
// goes. On a runner's screen it launches that runner. On a worker's it says something to that
// worker itself — which opens no job, so it never takes that worker's job place — and the worker
// it reaches is the one whose log is standing above it.
//
// What must not come back is a picker. A box that could send the words to a worker other than the
// one whose log is standing above it is a screen promising one thing and doing another, and the
// mistake it invites — reading one worker's log while the words go to another worker — leaves no trace on the
// screen that made it. Choosing a worker is walking to that worker's screen.
//
// This is read off the file that ships: which road a submission takes is decided in one
// expression, and a rendered box would say nothing about what the request carries.

const source = await Bun.file(new URL("../../../apps/web/app/components/LaunchPrompt.vue", import.meta.url)).text();
const template = source.slice(0, source.indexOf("<script"));

describe("where the box sends what is typed in it", () => {
  test("there is source to read, so nothing below passes on an empty string", () => {
    expect(source.length).toBeGreaterThan(500);
    expect(template).toContain("boxLabel");
    expect(source).toContain('props.agent === "worker" ? t("agents.label.launch_say") : t("agents.label.launch_task")');
  });

  test("a word to a worker takes its own road and launches nothing", () => {
    expect(source).toContain('props.agent === "worker" ? "/api/worker-word" : "/api/launch"');
    expect(source).toContain("road.value");
    // The launch road is the runners' alone now, so nothing here names a job's model or a job at
    // all: a word is not a job and carries no model to run one on.
    expect(source).not.toContain("JOB_MODEL");
    expect(source).not.toContain('"auto"');
  });

  test("the words go to the worker whose screen the box stands on", () => {
    expect(source).toContain("{ text: message.text.trim(), worker: props.worker }");
  });

  test("and there is no choosing a different one", () => {
    // A select over the workers, in any spelling: what stands in this foot is what the run is
    // launched with — a depth — and never where the words are sent.
    expect(template).not.toContain("agents.label.group_worker");
    expect(source).not.toContain("useAgentCatalogue");
    expect(source).not.toContain("workerItems");
  });

  test("the two runners launch themselves and carry no worker at all", () => {
    // A search or a browse is one fixed agent of this machine; there is no checkout to name.
    expect(source).toContain("agent: props.agent");
    expect(source).toContain('depth: props.agent === "search" ? depth.value : undefined');
  });

  test("what was typed and not sent is kept per screen rather than per kind", () => {
    // Two workers share the kind «worker». Keyed by that, words meant for one would appear in
    // the other's box.
    expect(template).toContain('kind="launch"');
    expect(template).toContain(':draft-id="props.worker ?? props.agent"');
  });

  test("it is the room's own box, and only a word to a worker takes files", () => {
    expect(template).toContain("<ChatComposer");
    expect(template).toContain(":attachable=\"props.agent === 'worker'\"");
    expect(source).toContain('form.append("file", picked.file, picked.file.name)');
  });
});
