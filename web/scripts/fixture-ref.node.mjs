import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
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

// Stock cosmos drops the navigator node when X.fixture.tsx collides with a
// directory X/ — fixtures live inside their directory, named for the component.
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
  if (dir === resolve("src")) continue;
  const branch = dir.slice(resolve("src").length + 1);
  test(`${branch} does not collide with a same-name fixture`, () => {
    assert.equal(existsSync(`${dir}.fixture.tsx`), false);
  });
}

for (const fixture of ["studio/sources/AddSourceDialog.fixture.tsx"]) {
  test(`${fixture} boots the real core before interactive source conversion`, () => {
    const source = readFileSync(resolve("src", fixture), "utf8");
    assert.match(source, /<CoreStage>/);
  });
}
