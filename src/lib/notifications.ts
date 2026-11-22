import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { createNotificationCenter, type Notice } from './notification-center';
import { translator } from '../i18n';
import { readClientSetting, writeClientSetting, acceptBoolean, legacyBoolean } from './settings-store';
export const notificationsEnabled=ref(false);
export async function loadNotificationPreference(){
 notificationsEnabled.value=await readClientSetting('notifications',true,acceptBoolean,{key:'airport.notifications',parse:legacyBoolean});
}
export const currentNotice=ref<Notice|null>(null);
let timer:ReturnType<typeof setTimeout>|undefined;
const native=()=>typeof window!=='undefined'&&'__TAURI_INTERNALS__' in window;
export function dismissNotice(){clearTimeout(timer);currentNotice.value=null;}
export const notifications=createNotificationCenter({
 permission:async()=>native()&&await isPermissionGranted(),
 request:async()=>native()?await requestPermission():'denied',
 send:(title,body)=>invoke<void>('send_desktop_notification',{title,body}),
},notice=>{clearTimeout(timer);currentNotice.value=notice;timer=setTimeout(dismissNotice,notice.kind==='error'?8000:5000);},()=>notificationsEnabled.value);
export async function enableNotifications(enabled:boolean){
 if(enabled&&!await notifications.permission(true))return false;
 try{
  await writeClientSetting('notifications',enabled,'airport.notifications');
  notificationsEnabled.value=enabled;
  return true;
 }catch{
  await notifications.show({body:translator(()=>{const lang=document.documentElement.lang;return ['zh-CN','en','ja','ko','de','fr','ru'].includes(lang)?lang as import('../i18n').Locale:'en';}).t('error.preferencesNotSaved'),kind:'error',native:false});
  return false;
 }
}
export async function prepareNotifications(){if(notificationsEnabled.value)await notifications.permission(true);}
