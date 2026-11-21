import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { createNotificationCenter, type Notice } from './notification-center';
function readPreference(){try{return localStorage.getItem('airport.notifications')!=='false';}catch{return true;}}
export const notificationsEnabled=ref(readPreference());
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
 notificationsEnabled.value=enabled;
 try{localStorage.setItem('airport.notifications',String(enabled));}catch{/* Session preference still applies. */}
 return true;
}
export async function prepareNotifications(){if(notificationsEnabled.value)await notifications.permission(true);}
