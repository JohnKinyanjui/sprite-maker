<script lang="ts">
  import { Gamepad2, RotateCcw, Grid3X3, Crosshair, Box, ZoomIn, Gauge } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import type { Animation, Asset } from "$lib/types";

  let { animations, assets, selectedAnimation, active = true }: { animations: Animation[]; assets: Asset[]; selectedAnimation?: Animation; active?: boolean } = $props();
  let activeAnimationId = $state<string | undefined>();
  let activeFrame = $state(0);
  let playing = $state(true);
  let x = $state(320); let y = $state(180);
  let speed = $state(130); let spriteScale = $state(2); let zoom = $state(1);
  let grid = $state(true); let pivot = $state(false); let collision = $state(false);
  let fpsOverride = $state(0);
  const held = new Set<string>();

  $effect(() => { if (!activeAnimationId && selectedAnimation) activeAnimationId = selectedAnimation.id; });
  let activeAnimation = $derived(animations.find(animation => animation.id === activeAnimationId) ?? selectedAnimation ?? animations[0]);
  let currentAsset = $derived(assets.find(asset => asset.id === activeAnimation?.frames[activeFrame]?.assetId));

  $effect(() => {
    if (!active || !playing || !activeAnimation?.frames.length) return;
    const duration = activeAnimation.frames[activeFrame]?.durationMs ?? 1000 / (fpsOverride || activeAnimation.fps);
    const timer = window.setTimeout(() => activeFrame = activeFrame < activeAnimation.frames.length - 1 ? activeFrame + 1 : activeAnimation.looping ? 0 : activeFrame, duration);
    return () => window.clearTimeout(timer);
  });

  function switchByName(fragment: string) {
    const match = animations.find(animation => animation.name.toLowerCase().includes(fragment));
    if (match) { activeAnimationId = match.id; activeFrame = 0; }
  }
  function reset() { x=320;y=180;activeFrame=0;playing=true; }
  $effect(() => {
    if (!active) { held.clear(); return; }
    function down(event: KeyboardEvent) {
      if ((event.target as HTMLElement)?.matches("input,select,textarea")) return;
      const key=event.key.toLowerCase();
      if(["w","a","s","d","arrowup","arrowdown","arrowleft","arrowright"," ","shift"].includes(key))event.preventDefault();
      held.add(key);
      if(key===" ")switchByName("attack"); if(key==="shift")switchByName("dodge");
    }
    function up(event: KeyboardEvent) { held.delete(event.key.toLowerCase()); }
    window.addEventListener("keydown",down);window.addEventListener("keyup",up);
    let last=performance.now(),frame=0;
    function tick(now:number){const delta=Math.min((now-last)/1000,.04);last=now;let dx=0,dy=0;if(held.has("a")||held.has("arrowleft"))dx--;if(held.has("d")||held.has("arrowright"))dx++;if(held.has("w")||held.has("arrowup"))dy--;if(held.has("s")||held.has("arrowdown"))dy++;if(dx||dy){const length=Math.hypot(dx,dy);x=Math.max(20,Math.min(620,x+(dx/length)*speed*delta));y=Math.max(20,Math.min(340,y+(dy/length)*speed*delta));}frame=requestAnimationFrame(tick)}
    frame=requestAnimationFrame(tick);
    return()=>{window.removeEventListener("keydown",down);window.removeEventListener("keyup",up);cancelAnimationFrame(frame);};
  });
</script>

<section class="test-room">
  <header><div><h1>Playground harness</h1><p>Run sprites in a lightweight desktop game loop</p></div><button onclick={reset}><RotateCcw size={13}/> Reset</button></header>
  <div class="body">
    <aside>
      <div class="label">PLAYBACK</div>
      <label>Animation<select bind:value={activeAnimationId} onchange={()=>activeFrame=0}>{#each animations as animation}<option value={animation.id}>{animation.name}</option>{/each}</select></label>
      <label><span><Gauge size={12}/> Movement speed</span><input type="range" min="40" max="300" bind:value={speed}/><small>{speed} px/s</small></label>
      <label><span><ZoomIn size={12}/> Sprite scale</span><input type="range" min="1" max="6" step=".5" bind:value={spriteScale}/><small>{spriteScale}×</small></label>
      <label>FPS override<select bind:value={fpsOverride}><option value={0}>Animation default</option><option value={6}>6 FPS</option><option value={8}>8 FPS</option><option value={10}>10 FPS</option><option value={12}>12 FPS</option><option value={16}>16 FPS</option><option value={24}>24 FPS</option></select></label>
      <div class="label section">VIEW</div>
      <button class:active={grid} onclick={()=>grid=!grid}><Grid3X3 size={13}/><span>Grid</span><small>{grid?"On":"Off"}</small></button>
      <button class:active={pivot} onclick={()=>pivot=!pivot}><Crosshair size={13}/><span>Pivot</span><small>{pivot?"On":"Off"}</small></button>
      <button class:active={collision} onclick={()=>collision=!collision}><Box size={13}/><span>Bounds</span><small>{collision?"On":"Off"}</small></button>
      <label><span><ZoomIn size={12}/> Room zoom</span><input type="range" min=".75" max="1.5" step=".25" bind:value={zoom}/><small>{zoom}×</small></label>
    </aside>
    <div class="room-wrap">
      <div class="room-shell">
        <div class="room" class:grid style={`transform:scale(${zoom})`}>
          <div class="floor-detail one"></div><div class="floor-detail two"></div><div class="floor-detail three"></div>
          <div class="character" style={`transform:translate(${x}px,${y}px) scale(${spriteScale})`}>
            {#if currentAsset}<img src={assetUrl(currentAsset.path)} alt="Playable character"/>{:else}<div class="placeholder"><Gamepad2 size={17}/></div>{/if}
            {#if pivot}<span class="pivot"><i></i></span>{/if}{#if collision}<span class="bounds"></span>{/if}
          </div>
        </div>
      </div>
      <div class="hud"><div><kbd>WASD</kbd><span>Move</span></div><div><kbd>SPACE</kbd><span>Attack</span></div><div><kbd>SHIFT</kbd><span>Dodge</span></div><div class="playing"><span></span>{activeAnimation?.name ?? "No animation"} · frame {activeAnimation?.frames.length ? activeFrame+1 : 0}</div></div>
    </div>
  </div>
</section>

<style>
  .test-room{height:100%;display:flex;flex-direction:column;background:var(--bg)}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 14px 0 17px}header h1{font-size:12px;margin:0}header p{font-size:11px;color:var(--faint);margin:3px 0 0}header button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:6px;padding:0 9px;font:inherit;font-size:11px;cursor:pointer}.body{flex:1;min-height:0;display:grid;grid-template-columns:195px minmax(0,1fr)}aside{border-right:1px solid var(--border);background:var(--sidebar);padding:14px 12px;overflow:auto}.label{font-size:10px;font-weight:700;letter-spacing:.13em;color:var(--faint);padding:3px 3px 11px}.label.section{margin-top:22px}aside label{font-size:11px;color:var(--muted);display:flex;flex-direction:column;gap:7px;margin:0 3px 16px}aside label>span{display:flex;gap:6px;align-items:center}aside label small{font-size:10px;color:var(--faint);text-align:right;margin-top:-5px}aside select{height:27px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}aside input[type="range"]{accent-color:var(--accent);height:3px}aside>button{width:100%;height:29px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:14px 1fr auto;gap:7px;align-items:center;padding:0 6px;text-align:left;font:inherit;font-size:11px;cursor:pointer}aside>button.active,aside>button:hover{background:var(--selected);color:var(--text)}aside>button small{font-size:10px;color:var(--faint)}
  .room-wrap{min-width:0;min-height:0;display:flex;flex-direction:column;background:#111316}.room-shell{flex:1;min-height:0;display:grid;place-items:center;overflow:hidden}.room{position:relative;width:640px;height:360px;transform-origin:center;border:1px solid #30343b;box-shadow:0 25px 70px #0008;background:#202521;overflow:hidden}.room.grid{background-color:#202521;background-image:linear-gradient(#31372f 1px,transparent 1px),linear-gradient(90deg,#31372f 1px,transparent 1px);background-size:32px 32px}.floor-detail{position:absolute;border:1px solid #394038;background:#282e27}.floor-detail.one{width:100px;height:55px;left:45px;top:44px}.floor-detail.two{width:74px;height:85px;right:62px;bottom:49px}.floor-detail.three{width:42px;height:42px;right:140px;top:55px;border-radius:50%}.character{position:absolute;left:0;top:0;width:1px;height:1px;transform-origin:center;will-change:transform}.character img{position:absolute;left:50%;bottom:0;transform:translateX(-50%);max-width:80px;max-height:80px;image-rendering:pixelated}.placeholder{position:absolute;width:30px;height:30px;left:-15px;bottom:0;background:#69786c;display:grid;place-items:center}.pivot{position:absolute;width:10px;height:10px;left:-5px;top:-5px;border:1px solid #f3be62;border-radius:50%;z-index:3}.pivot:before,.pivot:after{content:"";position:absolute;background:#f3be62}.pivot:before{width:14px;height:1px;left:-2px;top:4px}.pivot:after{width:1px;height:14px;left:4px;top:-2px}.bounds{position:absolute;width:34px;height:50px;left:-17px;bottom:0;border:1px solid #64b5df;background:#64b5df12;z-index:2}.hud{height:50px;border-top:1px solid var(--border);background:var(--sidebar);display:flex;align-items:center;padding:0 14px;gap:18px}.hud>div{display:flex;align-items:center;gap:6px;font-size:10px;color:var(--faint)}kbd{font:inherit;font-size:10px;color:var(--muted);border:1px solid var(--border-strong);background:var(--surface);border-radius:3px;padding:3px 5px}.hud .playing{margin-left:auto}.playing>span{width:6px;height:6px;border-radius:50%;background:#5ba57a}
  @media(max-width:1150px){.room{transform:scale(.8)!important}.body{grid-template-columns:175px minmax(0,1fr)}}
</style>
