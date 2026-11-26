type Session = { accessToken: string; refreshToken: string; revision: number };
type Tokens = Pick<Session, "accessToken" | "refreshToken">;
const refreshPath = "/api/v1/app/auth/refresh";

/** Refresh tokens rotate once: concurrent 401s must share the same renewal. */
export function createAuthenticatedApi(deps: {
  baseUrl(): string;
  session(): Session;
  replaceSession(tokens: Tokens): void;
  persistSession(): Promise<void>;
  fetch?: typeof fetch;
}) {
  const send = deps.fetch ?? fetch;
  let renewing: { revision: number; origin: string; token: string; promise: Promise<void> } | undefined;
  const origin = () => deps.baseUrl().replace(/\/$/, "");
  const sameSession = (revision: number, base: string) => deps.session().revision === revision && origin() === base;
  const network = (url: string, init: RequestInit) =>
    send(url, init).catch(() => {
      throw new Error("error.network");
    });

  function renew(session: Session, base: string): Promise<void> {
    if (renewing?.revision === session.revision && renewing.origin === base && renewing.token === session.refreshToken)
      return renewing.promise;
    const work = (async () => {
      const res = await network(base + refreshPath, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ refreshToken: session.refreshToken }),
      });
      // Logout, a new login or a different server invalidates pending renewal results.
      if (!sameSession(session.revision, base) || deps.session().refreshToken !== session.refreshToken)
        throw new Error("error.requestFailed");
      if (!res.ok) {
        if (res.status === 401 || res.status === 403) {
          deps.replaceSession({ accessToken: "", refreshToken: "" });
          await deps.persistSession();
        }
        throw new Error("error.requestFailed");
      }
      const body = await res.json();
      if (!sameSession(session.revision, base) || deps.session().refreshToken !== session.refreshToken)
        throw new Error("error.requestFailed");
      const tokens = body?.data;
      if (
        typeof tokens?.accessToken !== "string" ||
        !tokens.accessToken ||
        typeof tokens?.refreshToken !== "string" ||
        !tokens.refreshToken
      )
        throw new Error("error.requestFailed");
      deps.replaceSession({ accessToken: tokens.accessToken, refreshToken: tokens.refreshToken });
      await deps.persistSession();
    })();
    const entry = {
      revision: session.revision,
      origin: base,
      token: session.refreshToken,
      promise: work,
    };
    renewing = entry;
    // Use both handlers so cleanup cannot create an unhandled rejection.
    void work.then(
      () => {
        if (renewing === entry) renewing = undefined;
      },
      () => {
        if (renewing === entry) renewing = undefined;
      }
    );
    return work;
  }

  return async function api(path: string, init: RequestInit = {}): Promise<any> {
    const base = origin(),
      initial = deps.session();
    const request = async (retry: boolean): Promise<any> => {
      if (!sameSession(initial.revision, base)) throw new Error("error.requestFailed");
      const session = deps.session();
      const headers = new Headers(init.headers);
      headers.set("content-type", "application/json");
      if (session.accessToken) headers.set("authorization", `Bearer ${session.accessToken}`);
      const res = await network(base + path, { ...init, headers });
      if (!sameSession(initial.revision, base)) throw new Error("error.requestFailed");
      if (res.status === 401 && retry && path !== refreshPath && deps.session().refreshToken) {
        // A delayed 401 may arrive after another request already rotated the pair.
        if (deps.session().accessToken === session.accessToken) await renew(deps.session(), base);
        return request(false);
      }
      const body = await res.json();
      if (!sameSession(initial.revision, base)) throw new Error("error.requestFailed");
      if (!res.ok) throw new Error(body.message || "error.requestFailed");
      return body.data;
    };
    return request(true);
  };
}
