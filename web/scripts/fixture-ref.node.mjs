import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { resolveFixtureRef } from "./fixture-ref.mjs";

const manifest = {
  fixtures: [
    {
      filePath: "src/studio/StudioLayout.fixture.tsx",
      cleanPath: ["src", "studio", "StudioLayout"],
      rendererUrl: "renderer.html?fixtureId=base&locked=true",
    },
  ],
};

test("resolves a source-shaped component path and named composition", () => {
  assert.deepEqual(resolveFixtureRef(manifest, "studio/StudioLayout#Composed"), {
    path: "src/studio/StudioLayout.fixture.tsx",
    name: "Composed",
  });
});

test("accepts the exact fixture source path", () => {
  assert.deepEqual(resolveFixtureRef(manifest, "src/studio/StudioLayout.fixture.tsx#LiveCore"), {
    path: "src/studio/StudioLayout.fixture.tsx",
    name: "LiveCore",
  });
});

test("rejects an unknown component path", () => {
  assert.throws(() => resolveFixtureRef(manifest, "studio/Missing#Default"), /Unknown fixture/);
});

function fixtureDirs(dir) {
  const entries = readdirSync(dir, { withFileTypes: true });
  const nested = entries
    .filter((entry) => entry.isDirectory())
    .flatMap((entry) => fixtureDirs(resolve(dir, entry.name)));
  return entries.some((entry) => entry.isFile() && entry.name.endsWith(".fixture.tsx"))
    ? [dir, ...nested]
    : nested;
}

for (const dir of fixtureDirs(resolve("src"))) {
  const branch = dir.slice(resolve("src").length + 1);
  test(`${branch} is expandable and exposes its site composition as a child`, () => {
    assert.equal(existsSync(resolve(dir, "__COMPOSITION.fixture.tsx")), true);
    assert.equal(existsSync(resolve(dir, "Composition.fixture.tsx")), false);
    assert.equal(existsSync(resolve("src", `${branch}.fixture.tsx`)), false);
  });
}
