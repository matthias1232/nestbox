const form=document.querySelector('#recipe');
const host=document.querySelector('#hostname');
const network=document.querySelector('#network');
const compute=document.querySelector('#compute');
const output=document.querySelector('#output');
function render(){output.textContent='# Nestbox Compute seed bundle - generated locally\nhostname: '+host.value+'\nnetwork:\n  address: '+network.value+'\ncompute:\n  profile: '+compute.value+'\n\nruncmd:\n  - nestbox init --profile ' + compute.value + '\n  - nestbox policy apply /opt/nestbox/policy.yaml';}
form.addEventListener('submit',function(e){e.preventDefault();render();document.querySelector('#generate').scrollIntoView({behavior:'smooth'});});
document.querySelector('#copy').addEventListener('click',async function(){render();await navigator.clipboard.writeText(output.textContent);this.textContent='Copied';});
document.querySelector('#download').addEventListener('click',function(){render();const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([output.textContent]));a.download=host.value+'.seed.yaml';a.click();});render();
