<script lang="ts">
  import { ArrowRight, Bone, Layers3, Play, WandSparkles, X } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { AnimationPolishMode, Asset } from "$lib/types";

  let { asset, onContinue, onRig, onClose }: {
    asset: Asset;
    onContinue: (motion: string, polishMode: AnimationPolishMode) => void | Promise<void>;
    onRig?: () => void;
    onClose: () => void;
  } = $props();

  let motion = $state("");
  let polishMode = $state<AnimationPolishMode>("rig");
  let busy = $state(false);
  const lower = $derived(asset.name.toLowerCase());
  const suggestions = $derived(
    /rabbit|bunny|hare/.test(lower) ? ["Hop forward", "Run with bounding leaps", "Idle with ear and nose motion", "Pounce"] :
    /centipede|millipede|worm/.test(lower) ? ["Crawl with a leg wave", "Scuttle quickly", "Rear up and attack", "Idle antenna motion"] :
    /(?:^|[^a-z])(?:bird|bat|wing)(?:$|[^a-z])/.test(lower) ? ["Fly with a wing cycle", "Take off", "Land", "Idle wing flutter"] :
    /axe|bow|dagger|mace|staff|sword|spear|hammer|weapon/.test(lower) ? ["Swing or strike", "Raise and ready", "Recoil after impact", "Idle glint"] :
    /tree|plant|flower|grass|bush/.test(lower) ? ["Sway in the wind", "Rustle gently", "React to an impact", "Grow or bloom"] :
    /chest|box/.test(lower) ? ["Open and settle", "Close", "Bounce when unlocked", "Shake while locked"] :
    /door|gate/.test(lower) ? ["Open on its hinge", "Close", "Rattle", "Break apart"] :
    asset.category === "creatures" ? ["Walk using its natural gait", "Run", "Attack", "Idle breathing"] :
    asset.category === "characters" ? ["Walk", "Run", "Idle breathing", "Attack"] :
    ["Idle motion", "Move naturally", "Activate or interact", "React to an impact"]
  );

  async function continueToChat() {
    if (!motion.trim() || busy) return;
    busy = true;
    try { await onContinue(motion.trim(), polishMode); } finally { busy = false; }
  }
</script>

<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="motion-title">
    <header><div><p>PREPARE ANIMATION</p><h2 id="motion-title">How should {asset.name} move?</h2></div><button onclick={onClose} title="Close"><X size={15}/></button></header>
    <div class="source"><div class="preview"><img src={assetUrl(asset.path)} alt={asset.name}/></div><div><strong>Source master</strong><span>{asset.name} · {asset.width}×{asset.height}</span><p>Sprite Studio keeps this exact identity and builds repeatable motion from a deterministic rig.</p></div></div>
    <div class="suggestions"><span>Suggested motion</span><div>{#each suggestions as suggestion}<button class:selected={motion===suggestion} onclick={()=>motion=suggestion}>{suggestion}</button>{/each}</div></div>
    <label>Movement description<textarea bind:value={motion} rows="3" placeholder="For example: hop forward with a compressed crouch, airborne arc, forefeet landing, then hind feet settling"></textarea></label>
    <fieldset>
      <legend>Finish quality</legend>
      <div class="polish-options">
        <button class:selected={polishMode==="rig"} onclick={()=>polishMode="rig"}><Bone size={16}/><div><strong>Rig only <em>Default</em></strong><p>Fast, crisp, deterministic frames. No image calls for animation poses.</p></div></button>
        <button class:selected={polishMode==="ai-polish"} onclick={()=>polishMode="ai-polish"}><WandSparkles size={16}/><div><strong>AI polish</strong><p>Render the rig first, then repair small joints and outline defects without changing poses.</p></div></button>
        <button class:selected={polishMode==="full-redraw"} onclick={()=>polishMode="full-redraw"}><Layers3 size={16}/><div><strong>Full redraw <em>Experimental</em></strong><p>Redraw rig frames with AI while keeping the rig's pose and timing as authority.</p></div></button>
      </div>
    </fieldset>
    {#if onRig}<button class="rig-alternative" onclick={onRig}><Bone size={15}/><div><strong>Adjust the rig yourself</strong><p>Open the native Rig editor to review joint points, bones, layering, and poses before rendering.</p></div></button>{/if}
    <footer><span class="spacer"></span><button onclick={onClose}>Cancel</button><button class="primary" disabled={!motion.trim()||busy} onclick={continueToChat}><Play size={12}/>{busy?"Preparing…":"Continue in chat"}<ArrowRight size={12}/></button></footer>
  </div>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:95;background:#000b;display:grid;place-items:center;padding:18px}.dialog{width:min(720px,calc(100vw - 36px));max-height:calc(100vh - 36px);overflow:auto;background:var(--surface);border:1px solid var(--border-strong);border-radius:11px;box-shadow:0 28px 90px #000d;padding:20px}header{display:flex;align-items:flex-start;justify-content:space-between}header p{font-size:9px;letter-spacing:.15em;font-weight:700;color:var(--faint);margin:0 0 7px}h2{font-size:18px;margin:0}header button{width:28px;height:28px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}.source{display:grid;grid-template-columns:104px minmax(0,1fr);gap:14px;align-items:center;margin-top:20px;padding:12px;border:1px solid var(--border);border-radius:8px;background:var(--bg)}.preview{height:92px;display:grid;place-items:center;border-radius:6px;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:14px 14px;background-position:0 0,0 7px,7px -7px,-7px 0}.preview img{max-width:88%;max-height:88%;object-fit:contain;image-rendering:pixelated}.source strong,.source span{display:block}.source strong{font-size:12px}.source span{font-size:10px;color:var(--muted);margin-top:4px}.source p{font-size:10px;line-height:1.5;color:var(--faint);margin:8px 0 0}.suggestions{margin-top:18px}.suggestions>span,label,legend{font-size:10px;color:var(--muted)}.suggestions>div{display:flex;flex-wrap:wrap;gap:6px;margin-top:7px}.suggestions button{height:28px;border:1px solid var(--border);border-radius:15px;background:var(--bg);color:var(--muted);font:inherit;font-size:10px;padding:0 10px;cursor:pointer}.suggestions button:hover,.suggestions button.selected{border-color:var(--accent);background:var(--accent-dim);color:var(--text)}label{display:block;margin-top:18px}textarea{display:block;width:100%;resize:vertical;min-height:76px;margin-top:7px;border:1px solid var(--border-strong);border-radius:7px;background:var(--bg);color:var(--text);font:inherit;font-size:12px;line-height:1.5;padding:10px;outline:none}textarea:focus{border-color:var(--accent)}fieldset{border:0;padding:0;margin:17px 0 0}.polish-options{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;margin-top:7px}.polish-options button{min-height:92px;display:flex;align-items:flex-start;gap:9px;padding:11px;border:1px solid var(--border);border-radius:8px;background:var(--bg);color:var(--muted);font:inherit;text-align:left;cursor:pointer}.polish-options button.selected{border-color:var(--accent);background:var(--accent-dim);color:var(--text)}.polish-options :global(svg){flex:0 0 auto;color:var(--accent)}.polish-options strong{display:block;font-size:11px}.polish-options em{font-style:normal;font-size:8px;color:var(--accent);margin-left:4px}.polish-options p{font-size:9px;line-height:1.45;margin:5px 0 0;color:var(--faint)}footer{display:flex;justify-content:flex-end;align-items:center;gap:7px;border-top:1px solid var(--border);padding-top:16px;margin-top:20px}footer .spacer{flex:1}footer button{height:32px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--muted);font:inherit;font-size:11px;padding:0 11px;display:flex;align-items:center;gap:6px;cursor:pointer}footer .primary{background:var(--accent);border-color:var(--accent);color:#10120c}.rig-alternative{display:flex;align-items:flex-start;gap:10px;width:100%;margin-top:10px;padding:12px;border:1px solid var(--border-strong);border-radius:8px;background:var(--bg);color:var(--text);font:inherit;text-align:left;cursor:pointer}.rig-alternative :global(svg){flex:0 0 auto;color:var(--accent);margin-top:1px}.rig-alternative strong{font-size:11px}.rig-alternative p{font-size:10px;line-height:1.5;color:var(--muted);margin:4px 0 0}button:disabled{opacity:.45;cursor:default}@media(max-width:700px){.polish-options{grid-template-columns:1fr}.source{grid-template-columns:78px minmax(0,1fr)}.preview{height:72px}}
</style>
