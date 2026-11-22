import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadStoredPreferences, migrateClientSetting, preferencesStoreKey, rememberStored, saveStoredPreferences, subscriptionsStoreKey, networkSettingsStoreKey, acceptBoolean, type SettingStore } from "../src/lib/settings-store.ts";
const legacy = JSON.stringify({version:1,password:"secret",accessToken:"secret",refreshToken:"secret",locale:"en",theme:"dark",connectionMode:"tun",routeMode:"global",activeProfileId:"existing"});
function memoryStore(initial: Record<string,unknown>={}): SettingStore & {saved:Record<string,unknown>} {
 const saved=structuredClone(initial);
 return {saved,async get(key){return saved[key];},async set(key,value){saved[key]=value;},async save(){}};
}
test("preferences migrate into a file and delete the browser copy only after readback", async()=>{
 const store=memoryStore();let cleared=false;
 const parsed=await loadStoredPreferences(store,()=>legacy,undefined,()=>{assert.ok(store.saved.preferences);cleared=true;});
 assert.equal(parsed?.activeProfileId,"existing");assert.equal(cleared,true);
 assert.equal(JSON.stringify(store.saved).includes("secret"),false);
 assert.equal(JSON.stringify(store.saved).includes("refreshToken"),false);
});
test("missing or broken native storage never falls back to the webview",async()=>{
 let reads=0,writes=0;const legacyRead=()=>{reads++;return legacy;};
 assert.equal(await loadStoredPreferences(null,legacyRead),null);
 assert.equal(reads,0);
 const store:SettingStore={async get(){throw new Error("disk read failed");},async set(){writes++;},async save(){}};
 const status={useStore:true};
 await assert.rejects(loadStoredPreferences(store,legacyRead,status),/disk read failed/);
 assert.equal(reads,0);assert.equal(status.useStore,false);
 const value=(await loadStoredPreferences(memoryStore(),()=>legacy))!;
 await assert.rejects(saveStoredPreferences(null,value),/preferencesNotSaved/);
 await assert.rejects(saveStoredPreferences(store,value,status),/preferencesNotSaved/);
 assert.equal(writes,0);
});
test("a failed native write preserves legacy data for a later migration",async()=>{
 let cleared=false;const store:SettingStore={async get(){return null;},async set(){throw new Error("disk full");},async save(){}};
 await assert.rejects(loadStoredPreferences(store,()=>legacy,undefined,()=>{cleared=true;}),/disk full/);
 assert.equal(cleared,false);
});
test("a failed readback never deletes the only durable copy",async()=>{
 let cleared=false;
 const store:SettingStore={async get(){return null;},async set(){},async save(){}};
 await assert.rejects(loadStoredPreferences(store,()=>legacy,undefined,()=>{cleared=true;}),/preferencesNotSaved/);
 assert.equal(cleared,false);
});
test("valid native preferences are authoritative and stale browser copies are cleaned",async()=>{
 const store=memoryStore({preferences:{version:1,locale:"fr",connectionMode:"manual"}});
 let cleared=false;
 const loaded=await loadStoredPreferences(store,()=>{throw new Error("must not read legacy");},undefined,()=>{cleared=true;});
 assert.equal(loaded?.locale,"fr");assert.equal(loaded?.connectionMode,"manual");assert.equal(cleared,true);
});
test("malformed native preferences are preserved without writing defaults",async()=>{
 const store=memoryStore({preferences:{version:999}});
 await assert.rejects(loadStoredPreferences(store,()=>legacy),/preferencesNotSaved/);
 assert.deepEqual(store.saved.preferences,{version:999});
});
test("ordinary settings migrate, verify and clear in order",async()=>{
 const store=memoryStore();let clears=0;
 const options={read:()=>"false",parse:(raw:string)=>raw==="false"?false:null,clear:()=>{clears++;assert.equal(store.saved.notifications,false);}};
 assert.equal(await migrateClientSetting(store,"notifications",true,acceptBoolean,options),false);
 assert.equal(clears,1);
 assert.equal(await migrateClientSetting(store,"notifications",true,acceptBoolean,{...options,read:()=>{throw new Error("native wins");}}),false);
 assert.equal(clears,2);
});
test("failed ordinary-setting migration cannot erase the browser copy",async()=>{
 let cleared=false;
 const store:SettingStore={async get(){return undefined;},async set(){throw new Error("read only");},async save(){}};
 await assert.rejects(migrateClientSetting(store,"ipVisible",false,acceptBoolean,{read:()=>"true",parse:()=>true,clear:()=>{cleared=true;}}),/read only/);
 assert.equal(cleared,false);
});
test("existing file subscriptions and network settings survive migration",async()=>{
 const store=memoryStore({[subscriptionsStoreKey]:[{id:"kept"}],[networkSettingsStoreKey]:{proxyPort:2080,allowLan:false}});
 const subscriptions=await rememberStored(store,subscriptionsStoreKey,[{id:"legacy"}],v=>Array.isArray(v)?v:null);
 const network=await rememberStored(store,networkSettingsStoreKey,{proxyPort:1},v=>v&&typeof v==="object"?v as {proxyPort:number}:null);
 assert.equal(subscriptions[0].id,"kept");assert.equal(network.proxyPort,2080);
});
test("desktop source never writes localStorage, sessionStorage or IndexedDB",()=>{
 const root=fileURLToPath(new URL("../src/",import.meta.url));
 function scan(dir:string):void {
  for(const item of readdirSync(dir,{withFileTypes:true})){
   const path=resolve(dir,item.name);if(item.isDirectory())scan(path);
   else if(/\.(ts|vue)$/.test(path)){
    const text=readFileSync(path,"utf8");
    assert.doesNotMatch(text,/(?:localStorage|sessionStorage)\s*\.\s*setItem\s*\(/,path);
    assert.doesNotMatch(text,/indexedDB\s*\.\s*open\s*\(/,path);
    if(!path.endsWith("settings-store.ts"))assert.doesNotMatch(text,/(?:localStorage|sessionStorage)\s*\./,path);
   }
  }
 }
 scan(root);
});
