export function resolveFixtureRef(manifest, ref) {
  const [pathRef, name] = ref.split("#", 2);
  const normalized = pathRef.replace(/^src\//, "").replace(/\.fixture\.tsx$/, "");
  const fixture = manifest.fixtures.find((candidate) => {
    const sourcePath = candidate.filePath.replace(/^src\//, "").replace(/\.fixture\.tsx$/, "");
    return candidate.filePath === pathRef || sourcePath === normalized;
  });
  if (!fixture) {
    throw new Error(`Unknown fixture: "${pathRef}"`);
  }
  return { path: fixture.filePath, name: name || undefined };
}
