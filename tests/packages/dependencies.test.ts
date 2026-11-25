import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";

// The dependency boundary on the TypeScript side: among other repositories only the base libraries are
// depended on or imported. Every import of every file under packages/ and tests/packages is read by Bun's own
// transpiler, and every dependency each package's manifest names is held to the same boundary.

const root = resolve(import.meta.dir, "..", "..");
const baseLibraries = new Set(["bmo-chat", "bmo-chat-client", "bmo-ui"]);
const sourceGlob = new Bun.Glob("**/*.{ts,tsx,mts,cts,js,mjs,cjs}");
const manifestGlob = new Bun.Glob("**/package.json");

function written(path: string): boolean {
  return !path.split(sep).includes("node_modules");
}

// The package a bare specifier names: its scope and name where it is scoped, its first part otherwise.
function packageOf(specifier: string): string {
  const parts = specifier.split("/");

  return specifier.startsWith("@") ? parts.slice(0, 2).join("/") : (parts[0] ?? specifier);
}

function anotherRepository(name: string): boolean {
  return !baseLibraries.has(name) && /^(bmo[-_]|@bmo([-_/]|$))/.test(name);
}

// Why an import from `file` crosses the boundary, or null where it stays inside it.
function importCrossing(file: string, specifier: string): string | null {
  if (specifier.startsWith(".") || specifier.startsWith("/")) {
    const target = resolve(dirname(file), specifier);

    return relative(root, target).startsWith("..") ? `${specifier} reaches outside the repository` : null;
  }

  const name = packageOf(specifier);

  return anotherRepository(name) ? `${name} is another repository's package` : null;
}

// Why a dependency a manifest at `manifest` names crosses the boundary, or null where it stays inside it.
function dependencyCrossing(manifest: string, name: string, version: string): string | null {
  if (baseLibraries.has(name)) {
    return null;
  }

  if (anotherRepository(name)) {
    return `${name} is another repository's package`;
  }

  const local = /^(file|link|portal):(.*)$/.exec(version);

  if (local !== null && relative(root, resolve(dirname(manifest), local[2] ?? "")).startsWith("..")) {
    return `${name} stands outside the repository, at ${local[2]}`;
  }

  return /^(git|github:|git\+|https?:)/.test(version) ? `${name} is fetched from ${version}` : null;
}

function sources(): string[] {
  return ["packages", join("tests", "packages")].flatMap((folder) =>
    [...sourceGlob.scanSync({ cwd: join(root, folder) })].filter(written).map((path) => join(root, folder, path)));
}

describe("the dependency boundary", () => {
  test("no file imports another repository but the base libraries", () => {
    const crossings: string[] = [];

    for (const file of sources()) {
      const transpiler = new Bun.Transpiler({ loader: file.endsWith("x") ? "tsx" : "ts" });

      for (const one of transpiler.scanImports(readFileSync(file, "utf8"))) {
        const why = importCrossing(file, one.path);

        if (why !== null) {
          crossings.push(`${relative(root, file)}: ${why}`);
        }
      }
    }

    expect(sources().length).toBeGreaterThan(0);
    expect(crossings).toEqual([]);
  });

  test("no package depends on another repository but the base libraries", () => {
    const crossings: string[] = [];
    const manifests = [...manifestGlob.scanSync({ cwd: join(root, "packages") })].filter(written).map((path) => join(root, "packages", path));

    for (const manifest of manifests) {
      const read = JSON.parse(readFileSync(manifest, "utf8")) as Record<string, unknown>;

      for (const table of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) {
        for (const [name, version] of Object.entries((read[table] ?? {}) as Record<string, string>)) {
          const why = dependencyCrossing(manifest, name, version);

          if (why !== null) {
            crossings.push(`${relative(root, manifest)}: ${why}`);
          }
        }
      }
    }

    expect(manifests.length).toBeGreaterThan(0);
    expect(crossings).toEqual([]);
  });

  test("a crossing is named", () => {
    const file = join(root, "packages", "agent", "one.ts");
    const manifest = join(root, "packages", "agent", "package.json");

    expect(importCrossing(file, "bmo-job/client")).not.toBeNull();
    expect(importCrossing(file, "@bmo/brain")).not.toBeNull();
    expect(importCrossing(file, "../../../elsewhere/file")).not.toBeNull();
    expect(importCrossing(file, "bmo-chat-client/socket")).toBeNull();
    expect(importCrossing(file, "@agency/support/roots")).toBeNull();
    expect(importCrossing(file, "../support/roots")).toBeNull();
    expect(dependencyCrossing(manifest, "bmo-brain", "workspace:*")).not.toBeNull();
    expect(dependencyCrossing(manifest, "shared", "file:../../../shared")).not.toBeNull();
    expect(dependencyCrossing(manifest, "fetched", "github:someone/fetched")).not.toBeNull();
    expect(dependencyCrossing(manifest, "bmo-ui", "file:../../../ui")).toBeNull();
    expect(dependencyCrossing(manifest, "@agency/support", "workspace:*")).toBeNull();
    expect(dependencyCrossing(manifest, "eventsource-parser", "^4.1.1")).toBeNull();
  });
});
