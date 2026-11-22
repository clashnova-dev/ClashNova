import assert from 'node:assert/strict';
import test from 'node:test';
import { createAuthenticatedApi } from '../src/lib/authenticated-api.ts';

const response = (data: unknown, status = 200) => new Response(JSON.stringify({ data }), { status });
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
function client(send: typeof fetch) {
  let session = { accessToken: 'old-access', refreshToken: 'old-refresh', revision: 0 };
  let base = 'https://example.test/';
  const saved: string[] = [];
  const api = createAuthenticatedApi({
    baseUrl: () => base,
    session: () => ({ ...session }),
    replaceSession: tokens => { session = { ...session, ...tokens }; },
    persistSession: async () => { saved.push(session.refreshToken); },
    fetch: send,
  });
  return { api, saved, session: () => session,
    logout() { session = { accessToken: '', refreshToken: '', revision: session.revision + 1 }; },
    login() { session = { accessToken: 'different-access', refreshToken: 'different-refresh', revision: session.revision + 1 }; },
    server() { base = 'https://different.test'; },
  };
}

test('concurrent expired requests rotate once and durably remember the new refresh token', async () => {
  const gate = deferred<Response>();
  let renewals = 0, retries = 0;
  const app = client(async (url, init) => {
    if (String(url).endsWith('/auth/refresh')) { renewals++; return gate.promise; }
    if (new Headers(init?.headers).get('authorization') === 'Bearer new-access') { retries++; return response('ok'); }
    return response(null, 401);
  });
  const first = app.api('/subscriptions'), second = app.api('/nodes');
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(renewals, 1);
  gate.resolve(response({ accessToken: 'new-access', refreshToken: 'new-refresh' }));
  assert.deepEqual(await Promise.all([first, second]), ['ok', 'ok']);
  assert.equal(retries, 2);
  assert.deepEqual(app.saved, ['new-refresh']);
});

test('a delayed 401 reuses the completed renewal instead of replaying a rotated token', async () => {
  const delayed = deferred<Response>();
  let renewals = 0;
  const app = client(async (url, init) => {
    if (String(url).endsWith('/auth/refresh')) { renewals++; return response({ accessToken: 'new-access', refreshToken: 'new-refresh' }); }
    if (new Headers(init?.headers).get('authorization') === 'Bearer new-access') return response('ok');
    if (String(url).endsWith('/delayed')) return delayed.promise;
    return response(null, 401);
  });
  const pending = app.api('/delayed');
  assert.equal(await app.api('/first'), 'ok');
  delayed.resolve(response(null, 401));
  assert.equal(await pending, 'ok');
  assert.equal(renewals, 1);
});

test('a late renewal cannot restore a logged-out account, overwrite a new login or use another server', async () => {
  for (const action of ['logout', 'login', 'server'] as const) {
    const gate = deferred<Response>(), started = deferred<void>();
    let requests = 0;
    const app = client(async url => {
      if (String(url).endsWith('/auth/refresh')) { started.resolve(); return gate.promise; }
      requests++; return response(null, 401);
    });
    const pending = app.api('/nodes');
    const rejected = assert.rejects(pending, /error.requestFailed/);
    await started.promise;
    app[action]();
    const expected = { ...app.session() };
    gate.resolve(response({ accessToken: 'new-access', refreshToken: 'new-refresh' }));
    await rejected;
    assert.deepEqual(app.session(), expected);
    assert.equal(requests, 1);
    assert.deepEqual(app.saved, []);
  }
});

test('transient renewal errors preserve the saved session; rejected tokens clear the durable session', async () => {
  for (const status of [401, 403, 500, 503]) {
    const app = client(async url => response(null, String(url).endsWith('/auth/refresh') ? status : 401));
    await assert.rejects(app.api('/nodes'), /error.requestFailed/);
    assert.equal(app.session().refreshToken, status < 500 ? '' : 'old-refresh');
    assert.deepEqual(app.saved, status < 500 ? [''] : []);
  }
});

test('renewal network failure and malformed token pairs do not destroy the session', async () => {
  for (const result of [null, { accessToken: 'incomplete' }, { accessToken: '', refreshToken: 'invalid' }, 'network']) {
    const app = client(async url => {
      if (!String(url).endsWith('/auth/refresh')) return response(null, 401);
      if (result === 'network') throw new TypeError('Failed to fetch');
      return response(result);
    });
    await assert.rejects(app.api('/nodes'));
    assert.equal(app.session().refreshToken, 'old-refresh');
    assert.deepEqual(app.saved, []);
  }
});

test('renewal and request retries are bounded when the server continues returning 401', async () => {
  let renewals = 0, requests = 0;
  const app = client(async url => {
    if (String(url).endsWith('/auth/refresh')) { renewals++; return response({ accessToken: 'new-access', refreshToken: 'new-refresh' }); }
    requests++; return response(null, 401);
  });
  await assert.rejects(app.api('/nodes'));
  assert.equal(renewals, 1);
  assert.equal(requests, 2);
});
