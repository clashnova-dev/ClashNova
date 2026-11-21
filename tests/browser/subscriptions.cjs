const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
(async () => {
 const b = await chromium.launch({channel:'chrome',headless:true});
 try {
  const p = await b.newPage({viewport:{width:960,height:720}});const errors=[];p.on('pageerror',e=>errors.push(e.message));
  await p.goto(process.env.REVIEW_CLIENT_ORIGIN || 'http://localhost:1420');
  await p.getByRole('button',{name:'使用通用订阅',exact:true}).click();
  for(const name of ['概览','节点','订阅','连接','日志','设置']) {
   await p.locator('.nav').getByRole('button',{name,exact:true}).click();
   await p.locator('.app-head h1').filter({hasText:name}).waitFor();
   assert(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  }
  await p.locator('.nav').getByRole('button',{name:'订阅',exact:true}).click();
  await p.getByRole('button',{name:'添加订阅',exact:true}).first().click();
  await p.getByPlaceholder('例如：日常使用').fill('回归测试订阅');
  const raw=JSON.stringify({outbounds:[{type:'vless',tag:'测试节点',server:'192.0.2.1',server_port:443,uuid:'00000000-0000-4000-8000-000000000001'}]});
  await p.locator('input[type=file]').setInputFiles({name:'fixture.json',mimeType:'application/json',buffer:Buffer.from(raw)});
  await p.getByRole('button',{name:'导入并启用',exact:true}).click();
  await p.locator('dialog').waitFor({state:'hidden'});
  await p.evaluate(()=>{
   window.reviewSaves=0;window.reviewFail=true;
   window.__TAURI_INTERNALS__={invoke:async(cmd)=>{
    if(cmd==='save_subscriptions'){window.reviewSaves++;await new Promise(r=>setTimeout(r,150));if(window.reviewFail)throw Error('模拟磁盘写入失败');}
    return null;
   }};
  });
  const remove=p.getByRole('button',{name:'移除 回归测试订阅',exact:true});
  await remove.evaluate(button=>{button.click();button.click();});
  await p.getByText('订阅移除失败：Error: 模拟磁盘写入失败',{exact:true}).waitFor();
  assert.equal(await p.evaluate(()=>window.reviewSaves),1);
  assert.equal(await remove.count(),1,'failed persistence keeps card available for retry');
  await p.evaluate(()=>{window.reviewFail=false;});
  await remove.click();await remove.waitFor({state:'hidden'});
  assert.equal(await p.evaluate(()=>window.reviewSaves),2);
  assert.deepEqual(errors,[]);
  console.log('PASS: desktop six pages, import, failed removal retains card, double-click guard, retry removal');
 }finally{await b.close();}
})().catch(e=>{console.error(e);process.exit(1)});
