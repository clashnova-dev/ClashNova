export type RuleSource = 'application' | 'builtin' | 'subscription' | 'fallback';
export type RuleGroup = { order: number; kind: string; values: string[]; target: string; source: RuleSource };
export type RuleRow = { id: string; order: number; kind: string; value: string; target: string; source: RuleSource };
const kinds: Record<string,string> = {
 domain:'DOMAIN',domain_suffix:'DOMAIN-SUFFIX',domain_keyword:'DOMAIN-KEYWORD',domain_regex:'DOMAIN-REGEX',
 ip_cidr:'IP-CIDR',source_ip_cidr:'SRC-IP-CIDR',ip_is_private:'IP-PRIVATE',source_ip_is_private:'SRC-IP-PRIVATE',
 process_name:'PROCESS-NAME',process_path:'PROCESS-PATH',process_path_regex:'PROCESS-REGEX',
 port:'PORT',port_range:'PORT-RANGE',source_port:'SRC-PORT',network:'NETWORK',protocol:'PROTOCOL',inbound:'INBOUND',
};
export function ruleGroups(rules: unknown, final: string, appCount: number, builtinCount: number): RuleGroup[] {
 const list = Array.isArray(rules) ? rules : [];
 const groups: RuleGroup[] = [];
 for (let index=0;index<list.length;index++) {
  const rule=list[index];
  if(!rule || typeof rule!=='object' || Array.isArray(rule))continue;
  const source: RuleSource=index<appCount?'application':index<appCount+builtinCount?'builtin':'subscription';
  const target=typeof rule.outbound==='string'?rule.outbound:typeof rule.action==='string'?rule.action:'route';
  const fields=Object.keys(rule).filter(key=>!['outbound','action'].includes(key));
  const simple=fields.length===1 && kinds[fields[0]];
  const value=rule[fields[0]];
  // A logical/conjunctive/negated rule must stay intact: splitting it changes its meaning.
  const values=simple?(Array.isArray(value)?value:[value]).map(value=>typeof value==='string'?value:JSON.stringify(value)):[JSON.stringify(Object.fromEntries(fields.map(key=>[key,rule[key]])))];
  groups.push({order:index+1,kind:simple?kinds[fields[0]]:'LOGICAL',values,target,source});
 }
 groups.push({order:list.length+1,kind:'FINAL',values:['*'],target:final||'select',source:'fallback'});
 return groups;
}

/** Count without allocating 100k row objects; create only the requested page. */
export function rulePage(
 groups: RuleGroup[], page: number, size: number, source: RuleSource|'all',
 matches?: (row: RuleRow)=>boolean,
): { rows: RuleRow[]; total: number; pages: number } {
 const offset=Math.max(0,page-1)*size, rows:RuleRow[]=[];
 let total=0;
 for(const group of groups){
  if(source!=='all'&&group.source!==source)continue;
  if(!matches){
   const start=Math.max(0,offset-total),end=Math.min(group.values.length,offset+size-total);
   for(let i=start;i<end;i++)rows.push({id:group.order+':'+i,order:group.order,kind:group.kind,value:group.values[i],target:group.target,source:group.source});
   total+=group.values.length;
   continue;
  }
  for(let i=0;i<group.values.length;i++){
   const row={id:group.order+':'+i,order:group.order,kind:group.kind,value:group.values[i],target:group.target,source:group.source};
   if(!matches(row))continue;
   if(total>=offset&&rows.length<size)rows.push(row);
   total++;
  }
 }
 return {rows,total,pages:Math.max(1,Math.ceil(total/size))};
}
