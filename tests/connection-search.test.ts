import test from 'node:test';
import assert from 'node:assert/strict';
import { connectionSearch, type SearchOptions } from '../src/lib/connection-search.ts';
const base: SearchOptions = { caseSensitive: false, wholeWord: false, regex: false };
test('search modes handle case, literal metacharacters, Unicode boundaries and repeat matches', () => {
 assert.ok(connectionSearch('Cursor', base).matches('/apps/cursor'));
 assert.ok(!connectionSearch('Cursor', {...base,caseSensitive:true}).matches('/apps/cursor'));
 assert.ok(connectionSearch('api.test', base).matches('api.test:443'));
 assert.ok(!connectionSearch('api.test', base).matches('apixTest'));
 const words=connectionSearch('api', {...base,wholeWord:true});
 assert.ok(words.matches('api.test:443')); assert.ok(!words.matches('rapid.test'));
 assert.ok(!connectionSearch('中文', {...base,wholeWord:true}).matches('中文软件'));
 assert.ok(connectionSearch('中文', {...base,wholeWord:true}).matches('/中文/'));
 const expression=connectionSearch('^api[12]\\.test:443$', {...base,regex:true});
 assert.ok(expression.matches('api1.test:443')); assert.ok(expression.matches('api2.test:443'));
 assert.ok(!expression.matches('api3.test:443'));
});
test('invalid regex is recoverable and empty search clears results filtering', () => {
 const invalid=connectionSearch('[', {...base,regex:true});
 assert.equal(invalid.error,true); assert.equal(invalid.matches('host'),false);
 assert.ok(connectionSearch('[', base).matches('name[1]'));
 assert.ok(connectionSearch('', {...base,regex:true}).matches('anything'));
});
