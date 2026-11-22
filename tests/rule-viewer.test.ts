import assert from 'node:assert/strict';
import test from 'node:test';
import {ruleGroups,rulePage} from '../src/lib/rule-viewer.ts';
import {CHINA_DOMAINS} from '../src/lib/china-domains.ts';
import {builtinRouteRules} from '../src/lib/routing-rules.ts';
import {connectionSearch} from '../src/lib/connection-search.ts';
test('production rules page keeps ordering and renders only a bounded page',()=>{
 const groups=ruleGroups([
  {process_name:['Example.exe'],outbound:'direct'},
  ...builtinRouteRules(true),
  {domain_suffix:['example.net'],outbound:'select'},
 ],'select',1,2);
 const first=rulePage(groups,1,100,'all');
 assert.equal(first.total,1+1+CHINA_DOMAINS.length+1+1);
 assert.equal(first.rows.length,100);
 assert.equal(first.rows[0].source,'application');assert.equal(first.rows[1].kind,'IP-PRIVATE');
 const end=rulePage(groups,first.pages,100,'all');
 assert.equal(end.rows.at(-1)?.kind,'FINAL');
 const subscription=rulePage(groups,1,100,'subscription');
 assert.equal(subscription.total,1);assert.equal(subscription.rows[0].value,'example.net');
});
test('compound, logical and negated conditions are never split into misleading OR rules',()=>{
 const rules=[
  {domain_suffix:['example.com'],network:'tcp',outbound:'select'},
  {domain_suffix:['example.net','example.org'],invert:true,action:'reject'},
  {type:'logical',mode:'and',rules:[{port:443},{network:'tcp'}],outbound:'select'},
 ];
 const groups=ruleGroups(rules,'direct',0,0);
 assert.deepEqual(groups.slice(0,3).map(g=>g.kind),['LOGICAL','LOGICAL','LOGICAL']);
 assert.equal(groups[1].values.length,1);assert.match(groups[1].values[0],/"invert":true/);
});
test('global mode has only application policies plus final routing',()=>{
 const groups=ruleGroups([{process_name:['Browser.exe'],action:'reject'}],'select',1,0);
 const page=rulePage(groups,1,100,'all');
 assert.equal(page.total,2);assert.equal(page.rows[0].target,'reject');assert.equal(page.rows[1].kind,'FINAL');
});
test('rule search supports case, word, regex and invalid patterns',()=>{
 const groups=ruleGroups([{domain_suffix:['api.example.com','EXAMPLE.net','other.test'],outbound:'select'}],'direct',0,0);
 function search(query:string,regex=false,caseSensitive=false){
  const match=connectionSearch(query,{regex,caseSensitive,wholeWord:false});
  return rulePage(groups,1,100,'all',row=>!match.error&&match.matches(row.value));
 }
 assert.equal(search('example').total,2);
 assert.equal(search('EXAMPLE',false,true).total,1);
 assert.equal(search('^api\\.',true).total,1);
 assert.equal(search('[',true).total,0);
});
