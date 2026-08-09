<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { ChevronLeft, ChevronRight, Grid3X3, Pause, Play, Trash2, X } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage, type Animation, type Asset, type BackgroundJob, type FrameAlignment, type JobEvent, type SpriteSheet, type SpriteSheetLayout } from "$lib/types";

  let { workspaceId, worktreeId, animations, assets, active, onError, onNotice }: {
    workspaceId: string; worktreeId?: string; animations: Animation[]; assets: Asset[]; active: boolean;
    onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();

  let sheets = $state<SpriteSheet[]>([]);
  let jobs = $state<BackgroundJob[]>([]);
  let selectedAnimationId = $state("");
  let selectedSheetId = $state("");
  let configuredAnimationId = $state("");
  let name = $state("");
  let layout = $state<SpriteSheetLayout>("grid");
  let frameWidth = $state(64);
  let frameHeight = $state(64);
  let padding = $state(0);
  let spacing = $state(0);
  let columns = $state(4);
  let scale = $state(1);
  let transparent = $state(true);
  let alignment = $state<FrameAlignment>("bottom_center");
  let pivotX = $state(0.5);
  let pivotY = $state(1);
  let playing = $state(false);
  let frameIndex = $state(0);
  let playbackSpeed = $state(1);
  let zoom = $state(4);

  const animation = $derived(animations.find(item=>item.id===selectedAnimationId));
  const selectedSheet = $derived(sheets.find(item=>item.id===selectedSheetId));
  const frameAssets = $derived(animation?.frames.map(frame=>assets.find(asset=>asset.id===frame.assetId)).filter((asset):asset is Asset=>Boolean(asset)) ?? []);
  const currentAsset = $derived(frameAssets[frameIndex]);
  const sheetJobs = $derived(jobs.filter(job=>job.kind==="sprite_sheet"));
  const runningJobs = $derived(sheetJobs.filter(job=>["queued","running","analyzing"].includes(job.status)));

  async function refresh() {
    try {
      [sheets,jobs]=await Promise.all([api.listSpriteSheets(workspaceId,worktreeId),api.listJobs(workspaceId,worktreeId)]);
      if(!selectedSheetId&&sheets.length)selectedSheetId=sheets[0].id;
    } catch(error){onError(errorMessage(error));}
  }

  function configureFor(animation:Animation) {
    const frameAssets=animation.frames.map(frame=>assets.find(asset=>asset.id===frame.assetId)).filter((asset):asset is Asset=>Boolean(asset));
    frameWidth=Math.max(1,...frameAssets.map(asset=>asset.width));
    frameHeight=Math.max(1,...frameAssets.map(asset=>asset.height));
    name=`${animation.name} Sheet`;
    columns=Math.min(4,Math.max(1,animation.frames.length));
    frameIndex=0;playing=false;configuredAnimationId=animation.id;
  }

  async function build() {
    if(!animation){onError("Select an animation first");return;}
    try {
      const job=await api.queueSpriteSheet({projectId:workspaceId,worktreeId,animationId:animation.id,name,layout,frameWidth,frameHeight,padding,spacing,columns,scale,transparent,alignment,pivotX,pivotY});
      jobs=[job,...jobs.filter(item=>item.id!==job.id)];onNotice("Sprite-sheet build queued");
    } catch(error){onError(errorMessage(error));}
  }

  async function cancel(job:BackgroundJob) {
    try{const updated=await api.cancelJob(job.id);jobs=jobs.map(item=>item.id===updated.id?updated:item);}
    catch(error){onError(errorMessage(error));}
  }

  async function remove(sheet:SpriteSheet) {
    if(!window.confirm(`Delete the exported revision “${sheet.name}”?`))return;
    try{await api.deleteSpriteSheet(sheet.id);sheets=sheets.filter(item=>item.id!==sheet.id);selectedSheetId=sheets[0]?.id??"";onNotice("Sprite-sheet revision deleted");}
    catch(error){onError(errorMessage(error));}
  }

  function step(delta:number){if(!frameAssets.length)return;frameIndex=(frameIndex+delta+frameAssets.length)%frameAssets.length;}

  $effect(()=>{
    if(!animations.some(item=>item.id===selectedAnimationId)){
      selectedAnimationId=animations[0]?.id??"";configuredAnimationId="";
    }
    if(selectedAnimationId&&selectedAnimationId!==configuredAnimationId){const selected=animations.find(item=>item.id===selectedAnimationId);if(selected)configureFor(selected);}
  });
  $effect(()=>{
    if(!playing||!animation||frameAssets.length<2)return;
    const timer=window.setInterval(()=>step(1),Math.max(16,1000/(animation.fps*playbackSpeed)));
    return()=>window.clearInterval(timer);
  });
  $effect(()=>{if(active){workspaceId;worktreeId;void refresh();}});

  onMount(()=>{
    const unlistenPromise=listen<JobEvent>("job-event",async({payload})=>{
      const job=payload.job;if(job.kind!=="sprite_sheet"||job.projectId!==workspaceId||((worktreeId??"")!==(job.worktreeId??"")))return;
      jobs=[job,...jobs.filter(item=>item.id!==job.id)];
      if(job.status==="completed"){sheets=await api.listSpriteSheets(workspaceId,worktreeId);selectedSheetId=sheets[0]?.id??"";onNotice("Sprite-sheet revision completed");}
      if(job.status==="failed")onError(job.errorMessage??"Sprite-sheet build failed");
    });
    return()=>{unlistenPromise.then(unlisten=>unlisten());};
  });
</script>

<section class="sheet-studio">
  <header>
    <div><h1>Sprite-sheet studio</h1><p>{sheets.length} revision{sheets.length===1?"":"s"} · native Rust packing</p></div>
    <button class="primary" onclick={build} disabled={!animation||runningJobs.length>0}><Grid3X3 size={14}/>{runningJobs.length?"Building…":"Build sheet"}</button>
  </header>
  {#if animations.length}
    <div class="workspace">
      <aside class="settings">
        <label>Source animation<select bind:value={selectedAnimationId}>{#each animations as item}<option value={item.id}>{item.name} · {item.frames.length}f</option>{/each}</select></label>
        <label>Name<input bind:value={name}/></label>
        <div class="pair"><label>Layout<select bind:value={layout}><option value="horizontal">Horizontal</option><option value="vertical">Vertical</option><option value="grid">Grid</option></select></label>{#if layout==="grid"}<label>Columns<input type="number" min="1" max="64" bind:value={columns}/></label>{/if}</div>
        <div class="pair"><label>Frame width<input type="number" min="1" max="4096" bind:value={frameWidth}/></label><label>Frame height<input type="number" min="1" max="4096" bind:value={frameHeight}/></label></div>
        <div class="pair"><label>Padding<input type="number" min="0" max="128" bind:value={padding}/></label><label>Spacing<input type="number" min="0" max="128" bind:value={spacing}/></label></div>
        <div class="pair"><label>Export scale<select bind:value={scale}><option value={1}>1×</option><option value={2}>2×</option><option value={3}>3×</option><option value={4}>4×</option><option value={8}>8×</option></select></label><label>Alignment<select bind:value={alignment}><option value="bottom_center">Bottom center</option><option value="center">Center</option><option value="top_left">Top left</option></select></label></div>
        <div class="pair"><label>Pivot X<input type="number" min="0" max="1" step="0.05" bind:value={pivotX}/></label><label>Pivot Y<input type="number" min="0" max="1" step="0.05" bind:value={pivotY}/></label></div>
        <label class="check"><input type="checkbox" bind:checked={transparent}/> Transparent background</label>
        <div class="jobs"><h2>Background jobs</h2>{#if sheetJobs.length}{#each sheetJobs.slice(0,6) as job}<article class:failed={job.status==="failed"}><div><strong>{job.stage}</strong><span>{Math.round(job.progress*100)}% · {job.status}</span></div><div class="bar"><i style={`width:${job.progress*100}%`}></i></div>{#if ["queued","running","analyzing"].includes(job.status)}<button onclick={()=>cancel(job)}><X size={11}/>Cancel</button>{/if}</article>{/each}{:else}<p>No jobs yet.</p>{/if}</div>
      </aside>
      <main class="preview-column">
        <div class="player">
          <div class="stage" class:pixel-grid={zoom>=8}>{#if currentAsset}<img src={assetUrl(currentAsset.path)} alt={currentAsset.name} style={`width:${currentAsset.width*zoom}px;height:${currentAsset.height*zoom}px`}/>{/if}</div>
          <div class="transport"><button onclick={()=>step(-1)} title="Previous frame"><ChevronLeft size={14}/></button><button class="play" onclick={()=>playing=!playing}>{#if playing}<Pause size={14}/>{:else}<Play size={14} fill="currentColor"/>{/if}</button><button onclick={()=>step(1)} title="Next frame"><ChevronRight size={14}/></button><input aria-label="Scrub frames" type="range" min="0" max={Math.max(0,frameAssets.length-1)} bind:value={frameIndex}/><span>{frameIndex+1}/{frameAssets.length}</span><select aria-label="Playback speed" bind:value={playbackSpeed}><option value={0.5}>0.5×</option><option value={1}>1×</option><option value={2}>2×</option></select><select aria-label="Preview zoom" bind:value={zoom}><option value={2}>2×</option><option value={4}>4×</option><option value={8}>8×</option><option value={12}>12×</option></select></div>
        </div>
        <section class="revisions">
          <div class="revision-head"><div><h2>Export revisions</h2><p>Every build is preserved with PNG and JSON frame metadata.</p></div>{#if selectedSheet}<button onclick={()=>revealItemInDir(selectedSheet.pngPath)}>Reveal files</button>{/if}</div>
          {#if sheets.length}<div class="revision-content"><nav>{#each sheets as sheet}<button class:active={sheet.id===selectedSheetId} onclick={()=>selectedSheetId=sheet.id}><strong>{sheet.name}</strong><span>{sheet.width}×{sheet.height} · {sheet.layout} · {sheet.frameCount}f</span></button>{/each}</nav>{#if selectedSheet}<div class="sheet-preview"><div><img src={assetUrl(selectedSheet.pngPath)} alt={selectedSheet.name}/></div><footer><span>{selectedSheet.columns} cols × {selectedSheet.rows} rows</span><span>{selectedSheet.frameWidth}×{selectedSheet.frameHeight} cells · {selectedSheet.scale}× export</span><button onclick={()=>remove(selectedSheet)} title="Delete revision"><Trash2 size={12}/></button></footer></div>{/if}</div>{:else}<div class="empty"><Grid3X3 size={26}/><strong>No sprite-sheet revisions</strong><p>Choose an animation, configure its cells, then build a versioned export.</p></div>{/if}
        </section>
      </main>
    </div>
  {:else}<div class="empty whole"><Grid3X3 size={28}/><strong>Create an animation first</strong><p>Sprite sheets are built directly from saved animation timelines.</p></div>{/if}
</section>

<style>
  .sheet-studio{height:100%;display:flex;flex-direction:column;background:var(--bg)}header{height:58px;min-height:58px;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 16px}h1{font-size:14px;margin:0}header p,.revision-head p{font-size:10px;color:var(--faint);margin:4px 0 0}.primary,.revision-head button{height:31px;border:1px solid var(--text);background:var(--text);color:var(--bg);border-radius:6px;padding:0 10px;display:flex;align-items:center;gap:6px;font:inherit;font-size:11px;cursor:pointer}.primary:disabled{opacity:.5;cursor:wait}.workspace{min-height:0;flex:1;display:grid;grid-template-columns:260px minmax(0,1fr)}.settings{overflow:auto;border-right:1px solid var(--border);background:var(--sidebar);padding:13px}.settings>label,.pair label{display:block;font-size:10px;color:var(--muted);margin-bottom:11px}.settings input,.settings select{display:block;width:100%;height:29px;margin-top:5px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;padding:0 7px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:7px}.check{display:flex!important;align-items:center;gap:7px;margin:4px 0 16px!important}.check input{display:block;width:14px;height:14px;margin:0}.jobs{border-top:1px solid var(--border);padding-top:13px}.jobs h2,.revision-head h2{font-size:11px;margin:0}.jobs>p{font-size:10px;color:var(--faint)}.jobs article{padding:8px 0;border-bottom:1px solid var(--border)}.jobs article>div:first-child{display:flex;justify-content:space-between;gap:6px}.jobs strong,.jobs span{font-size:9px}.jobs span{color:var(--faint)}.jobs .bar{height:3px;background:var(--selected);margin-top:7px}.jobs .bar i{display:block;height:100%;background:var(--accent)}.jobs article.failed .bar i{background:#cf7772}.jobs article button{margin-top:6px;border:0;background:transparent;color:var(--faint);font:inherit;font-size:9px;display:flex;align-items:center;gap:3px;cursor:pointer}.preview-column{min-width:0;min-height:0;display:grid;grid-template-rows:minmax(320px,1fr) 260px}.player{min-height:0;display:flex;flex-direction:column;align-items:center;justify-content:center;border-bottom:1px solid var(--border)}.stage{width:min(520px,70%);height:min(350px,72%);overflow:auto;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border:1px solid var(--border-strong)}.stage img{object-fit:contain;image-rendering:pixelated;max-width:none;max-height:none}.stage.pixel-grid img{outline:1px solid #ffffff10}.transport{display:flex;align-items:center;gap:5px;margin-top:11px}.transport button,.transport select{height:27px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;font:inherit;font-size:10px}.transport button{width:27px;display:grid;place-items:center;cursor:pointer}.transport button.play{background:var(--text);color:var(--bg)}.transport input{width:180px}.transport span{font-size:10px;color:var(--faint);min-width:38px;text-align:center}.revisions{min-height:0;background:var(--sidebar);display:flex;flex-direction:column}.revision-head{height:51px;min-height:51px;padding:0 13px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--border)}.revision-head button{height:27px;background:var(--surface);border-color:var(--border);color:var(--muted)}.revision-content{min-height:0;flex:1;display:grid;grid-template-columns:230px minmax(0,1fr)}.revision-content nav{overflow:auto;border-right:1px solid var(--border);padding:7px}.revision-content nav button{width:100%;border:1px solid transparent;border-radius:5px;background:transparent;color:var(--text);padding:8px;text-align:left;cursor:pointer}.revision-content nav button:hover,.revision-content nav button.active{background:var(--surface);border-color:var(--border)}.revision-content nav strong,.revision-content nav span{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.revision-content nav strong{font-size:10px}.revision-content nav span{font-size:9px;color:var(--faint);margin-top:4px}.sheet-preview{min-width:0;display:grid;grid-template-rows:minmax(0,1fr) 32px}.sheet-preview>div{min-height:0;overflow:auto;display:grid;place-items:center;background:var(--preview)}.sheet-preview img{max-width:94%;max-height:120px;image-rendering:pixelated;object-fit:contain}.sheet-preview footer{border-top:1px solid var(--border);display:flex;align-items:center;gap:14px;padding:0 10px;color:var(--faint);font-size:9px}.sheet-preview footer button{margin-left:auto;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;cursor:pointer}.empty{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;color:var(--faint);text-align:center}.empty strong{font-size:12px;color:var(--text);margin-top:10px}.empty p{font-size:10px;margin:5px 0}.empty.whole{height:100%}@media(max-width:1000px){.workspace{grid-template-columns:220px minmax(0,1fr)}.stage{width:84%}.transport input{width:100px}}
</style>
