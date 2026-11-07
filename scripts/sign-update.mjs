// Private signing material stays outside the published tree. Never regenerate an existing public key.
import {generateKeyPairSync,createPrivateKey,createPublicKey,createHash,sign,verify} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,existsSync,statSync} from 'node:fs';
import {resolve,basename} from 'node:path';
const root=resolve(import.meta.dirname,'..');
const publicPath=resolve(root,'assets/update-public-key.txt');
const keyPath=process.env.RUNBOOK_SIGNING_KEY || resolve(root,'.keys/update-private.pem');
if(process.argv.includes('--init')) {
  if(existsSync(publicPath)||existsSync(keyPath))throw Error('Signing key already exists; refusing to replace trust keys.');
  mkdirSync(resolve(root,'.keys'),{recursive:true});
  const keys=generateKeyPairSync('ed25519');
  writeFileSync(keyPath,keys.privateKey.export({type:'pkcs8',format:'pem'}),{mode:0o600,flag:'wx'});
  writeFileSync(publicPath,keys.publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64')+'\n',{flag:'wx'});
  console.log(`Created signing key: ${keyPath}\nBack it up securely; the app pins assets/update-public-key.txt.`);
} else {
  const argument=name=>{const i=process.argv.indexOf(name);return i<0 ? undefined : process.argv[i+1];};
  const version=JSON.parse(readFileSync(resolve(root,'package.json'))).version;
  const archive=resolve(argument('--archive') || resolve(root,`release/Runbook-${version}-windows-x64.zip`));
  const notes=argument('--notes') ? readFileSync(resolve(argument('--notes')),'utf8') : 'Runbook 更新';
  const key=createPrivateKey(process.env.RUNBOOK_SIGNING_KEY_PEM || readFileSync(keyPath));
  const rawPublic=createPublicKey(key).export({type:'spki',format:'der'}).subarray(-32).toString('base64');
  if(rawPublic!==readFileSync(publicPath,'utf8').trim())throw Error('Private key does not match the public key pinned by the app.');
  const payload=JSON.stringify({schema:1,app:'dev.runbook.desktop',version,platform:'windows-x86_64',data_schema:1,
    url:argument('--url') || `https://github.com/fishOfOUC/command_terminal/releases/download/v${version}/${basename(archive)}`,
    size:statSync(archive).size,sha256:createHash('sha256').update(readFileSync(archive)).digest('hex'),notes});
  const signature=sign(null,Buffer.from(payload),key);
  if(!verify(null,Buffer.from(payload),createPublicKey(key),signature))throw Error('Signature self-check failed');
  const output=resolve(argument('--output') || resolve(root,'release/latest.json'));
  writeFileSync(output,JSON.stringify({payload,signature:signature.toString('base64')},null,2)+'\n');
  console.log(`Signed update manifest: ${output}`);
}
