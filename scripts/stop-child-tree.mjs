import {spawn} from 'node:child_process';

// Stop descendants while their parent is still alive: killing only the app can orphan ConPTY hosts.
export async function stopChildTree(child) {
  if(!child.pid || child.exitCode!==null || child.signalCode!==null)return;
  if(process.platform!=='win32'){child.kill();return;}
  const killer=spawn('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true,stdio:'ignore'});
  const code=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{killer.kill();reject(Error(`Timed out stopping test process tree ${child.pid}`));},10000);
    killer.once('error',error=>{clearTimeout(timer);reject(error);});
    killer.once('close',code=>{clearTimeout(timer);resolve(code);});
  });
  if(code!==0 && child.exitCode===null && child.signalCode===null)throw Error(`Could not stop test process tree ${child.pid} (taskkill ${code})`);
}
