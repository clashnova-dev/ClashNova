// Build-time flags: stateless constants shared across stores / the facade.

// Hides the account / sign-in system when off; the client then runs as a pure
// subscription manager (build-time VITE_ENABLE_LOGIN).
export const enableLogin = (import.meta.env.VITE_ENABLE_LOGIN ?? "true").trim().toLowerCase() !== "false";

// About-page links moved to ./domains with the other domains; re-exported here so
// existing import sites keep working.
export { githubUrl } from "./domains";
