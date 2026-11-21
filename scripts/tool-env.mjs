// Windows environment keys are case-insensitive; Node child processes must not
// receive both Path and PATH, or one value can silently hide the other.
export function normalizeToolPath(env, platform = process.platform) {
  const next = { ...env };
  if (platform !== "win32") return next;
  const keys = Object.keys(next).filter(key => key.toLowerCase() === "path");
  const value = next.PATH ?? next.Path ?? next[keys[0]] ?? "";
  for (const key of keys) delete next[key];
  next.PATH = value;
  return next;
}
