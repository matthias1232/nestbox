const form=document.querySelector('#recipe');
const host=document.querySelector('#hostname');
const network=document.querySelector('#network');
const compute=document.querySelector('#compute');
const output=document.querySelector('#output');
function render(){output.textContent='# Nestbox Compute seed bundle - generated locally\nhostname: '+host.value+'\nnetwork:\n  address: '+network.value+'\ncompute:\n  profile: '+compute.value+'\n\nruncmd:\n  - nestbox init --profile ' + compute.value + '\n  - nestbox policy apply /opt/nestbox/policy.yaml';}
form.addEventListener('submit',function(e){e.preventDefault();render();document.querySelector('#generate').scrollIntoView({behavior:'smooth'});});
document.querySelector('#copy').addEventListener('click',async function(){render();await navigator.clipboard.writeText(output.textContent);this.textContent='Copied';});
document.querySelector('#download').addEventListener('click',function(){render();const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([output.textContent]));a.download=host.value+'.seed.yaml';a.click();});render();
const overlay = document.querySelector("#dev-overlay");
document.querySelector("#dismiss-overlay")?.addEventListener("click", () => { overlay?.remove(); });
const slides = [...document.querySelectorAll(".feature-slide")]; const dots = document.querySelector("#slider-dots"); const count = document.querySelector("#slide-count"); let activeSlide = 0;
function showSlide(index) { activeSlide=(index+slides.length)%slides.length; slides.forEach((slide,i)=>slide.classList.toggle("active",i===activeSlide)); [...dots.children].forEach((dot,i)=>dot.classList.toggle("active",i===activeSlide)); if(count)count.textContent=String(activeSlide+1).padStart(2,"0")+" / "+String(slides.length).padStart(2,"0"); }
slides.forEach((_,i)=>{const dot=document.createElement("button");dot.type="button";dot.setAttribute("aria-label","Show feature "+(i+1));dot.addEventListener("click",()=>showSlide(i));dots?.append(dot);});
document.querySelector("#prev-slide")?.addEventListener("click",()=>showSlide(activeSlide-1)); document.querySelector("#next-slide")?.addEventListener("click",()=>showSlide(activeSlide+1)); if(slides.length)showSlide(0);
let sliderTouchStart = null;
document.querySelector("#feature-slider")?.addEventListener("touchstart", event => { sliderTouchStart = event.changedTouches[0].clientX; }, { passive: true });
document.querySelector("#feature-slider")?.addEventListener("touchend", event => { const delta = event.changedTouches[0].clientX - sliderTouchStart; if (Math.abs(delta) > 45) showSlide(activeSlide + (delta < 0 ? 1 : -1)); }, { passive: true });
const intelligence = document.querySelector('#hero-intelligence');
document.querySelectorAll('.language-option').forEach(button => {
  button.addEventListener('click', () => {
    const usa = button.dataset.variant === 'usa';
    intelligence.textContent = usa ? 'SI' : 'AI';
    document.querySelectorAll('.language-option').forEach(option => {
      const selected = option === button;
      option.classList.toggle('active', selected);
      option.setAttribute('aria-pressed', String(selected));
    });
  });
});
const aiTextNodes = [];
const textWalker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
let aiTextNode;
while ((aiTextNode = textWalker.nextNode())) {
  if (/\bAI\b/.test(aiTextNode.nodeValue) && !aiTextNode.parentElement.closest('script, style')) aiTextNodes.push([aiTextNode, aiTextNode.nodeValue]);
}
function setSiteVariant(usa) {
  aiTextNodes.forEach(([node, original]) => { node.nodeValue = usa ? original.replace(/\bAI\b/g, 'SI') : original; });
}
document.querySelectorAll('.language-option').forEach(button => button.addEventListener('click', () => setSiteVariant(button.dataset.variant === 'usa')));
