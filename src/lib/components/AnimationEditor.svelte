<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Play, Pause, SkipBack, SkipForward, Plus, Copy, Trash2, GripVertical, Save, Download, Clapperboard, Repeat2, FileKey2, ShieldCheck } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import TemplateDialog from "$lib/components/TemplateDialog.svelte";
  import TemplateApplyDialog from "$lib/components/TemplateApplyDialog.svelte";
  import QualityPanel from "$lib/components/QualityPanel.svelte";
  import { errorMessage, type Animation, type AnimationFrame, type AnimationTemplate, type Asset, type BackgroundJob, type FrameMode, type JobEvent, type QualityCheck, type QualityReport, type TemplateApplication } from "$lib/types";

  let { workspaceId, worktreeId, assets, animations, templates, selectedAnimation, active = true, onAnimations, onTemplates, onSelected, onTemplateApplication, onError, onNotice }: {
    workspaceId: string; worktreeId?: string; assets: Asset[]; animations: Animation[]; templates: AnimationTemplate[]; selectedAnimation?: Animation;
    active?: boolean; onAnimations: (animations: Animation[]) => void; onTemplates: (templates: AnimationTemplate[]) => void;
    onSelected: (animation: Animation) => void; onTemplateApplication: (application:TemplateApplication)=>void;
    onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();
  let selectedPropId = $state<string | undefined>();
  let animationId = $state<string | undefined>();
  let name = $state("New animation");
  let fps = $state(10);
  let looping = $state(true);
  let frames = $state<AnimationFrame[]>([]);
  let activeFrame = $state(0);
  let playing = $state(false);
  let scale = $state(2);
  let onionSkin = $state(false);
  let onionOpacity = $state(0.24);
  let saving = $state(false);
  let draggedIndex = $state<number | undefined>();
  let templateDialog = $state(false);
  let applyingTemplate = $state<AnimationTemplate>();
  let templateBusy = $state(false);
  let qualityOpen = $state(false);
  let qualityReport = $state<QualityReport>();
  let qualityJob = $state<BackgroundJob>();
  let repairing = $state(false);
  let optimizing = $state(false);

  $effect(() => {
    if (selectedAnimation && selectedAnimation.id !== selectedPropId) loadAnimation(selectedAnimation);
  });
  $effect(() => {
    if (!active || !playing || !frames.length) return;
    const duration = frames[activeFrame]?.durationMs ?? 1000 / fps;
    const timer = window.setTimeout(() => {
      if (activeFrame < frames.length - 1) activeFrame += 1;
      else if (looping) activeFrame = 0;
      else playing = false;
    }, duration);
    return () => window.clearTimeout(timer);
  });

  let currentAsset = $derived(assets.find(asset => asset.id === frames[activeFrame]?.assetId));
  let previousAsset = $derived(activeFrame > 0 ? assets.find(asset => asset.id === frames[activeFrame-1]?.assetId) : looping && frames.length > 1 ? assets.find(asset => asset.id === frames.at(-1)?.assetId) : undefined);
  let nextAsset = $derived(activeFrame < frames.length-1 ? assets.find(asset => asset.id === frames[activeFrame+1]?.assetId) : looping && frames.length > 1 ? assets.find(asset => asset.id === frames[0]?.assetId) : undefined);
  const frameAsset = (frame: AnimationFrame) => assets.find(asset => asset.id === frame.assetId);
  const frameSeverity = (index:number) => qualityReport?.checks.some(check=>check.frameIndex===index&&check.severity==="error"&&!check.ignored)?"error":qualityReport?.checks.some(check=>check.frameIndex===index&&check.severity==="warning"&&!check.ignored)?"warning":qualityReport?"good":"";
  let canOptimize = $derived(Boolean(animationId&&qualityReport?.checks.some(check=>!check.ignored&&check.severity!=="info"&&["remove_duplicate","regenerate_transition"].includes(check.repairAction??""))));

  function loadAnimation(animation:Animation){selectedPropId=animation.id;animationId=animation.id;name=animation.name;fps=animation.fps;looping=animation.looping;frames=animation.frames.map(frame=>({...frame}));activeFrame=0;playing=false;qualityReport=undefined;qualityJob=undefined;void loadQuality(animation.id);}
  function startNewAnimationDraft() { animationId=undefined;name="New animation";fps=10;looping=true;frames=[];activeFrame=0;playing=false;qualityOpen=false;qualityReport=undefined;qualityJob=undefined; }
  function selectAnimation(animation:Animation){loadAnimation(animation);onSelected(animation);}
  function addFrame(assetId: string) { frames = [...frames, {assetId}]; activeFrame = frames.length - 1; }
  function removeFrame(index: number) { frames = frames.filter((_, i) => i !== index); activeFrame = Math.max(0, Math.min(activeFrame, frames.length - 1)); }
  function duplicate(index: number) { frames = [...frames.slice(0,index+1), {...frames[index]}, ...frames.slice(index+1)]; activeFrame=index+1; }
  function move(from: number, to: number) { if(from===to||to<0||to>=frames.length)return;const next=[...frames];const [item]=next.splice(from,1);next.splice(to,0,item);frames=next;activeFrame=to; }
  function drop(event: DragEvent, index?: number) {
    event.preventDefault();
    if (draggedIndex !== undefined && index !== undefined) { move(draggedIndex,index); draggedIndex=undefined; return; }
    const id = event.dataTransfer?.getData("application/x-sprite-studio-asset") || event.dataTransfer?.getData("text/plain");
    if (id && assets.some(asset => asset.id === id)) {
      if(index === undefined) addFrame(id); else { frames=[...frames.slice(0,index),{assetId:id},...frames.slice(index)];activeFrame=index; }
    }
  }
  async function save() {
    saving=true;
    try {
      const animation=await api.saveAnimation({id:animationId,workspaceId,worktreeId,name,fps:Number(fps),looping,frames});
      selectedPropId=animation.id;animationId=animation.id;onSelected(animation);onAnimations(await api.listAnimations(workspaceId,worktreeId));onNotice("Animation saved");
    } catch(error){onError(errorMessage(error));} finally{saving=false;}
  }
  async function exportSheet() {
    if(!animationId){onError("Save the animation before exporting");return;}
    const destination=await open({directory:true,multiple:false,title:"Export spritesheet"});
    if(typeof destination!=="string")return;
    try{const result=await api.exportAnimation(animationId,destination);onNotice(`Exported ${result.width}×${result.height} spritesheet`);await revealItemInDir(result.pngPath);}
    catch(error){onError(errorMessage(error));}
  }
  async function createTemplate(value:{name:string;intent:string;motionDescription:string;frameMode:FrameMode;minFrames:number;maxFrames:number;generationPrompt:string;negativePrompt:string}) {
    if(!animationId)return;
    templateBusy=true;
    try{await api.createAnimationTemplate(animationId,value.name,value.intent,value.motionDescription,value.frameMode,value.minFrames,value.maxFrames,value.generationPrompt,value.negativePrompt);onTemplates(await api.listAnimationTemplates(workspaceId));templateDialog=false;onNotice("Reusable motion template saved");}
    catch(error){onError(errorMessage(error));}finally{templateBusy=false;}
  }
  async function prepareTemplate(targetAssetId:string){if(!applyingTemplate)return;templateBusy=true;try{const application=await api.applyAnimationTemplate(applyingTemplate.id,targetAssetId);onTemplateApplication(application);applyingTemplate=undefined;onNotice(`Prepared ${application.motionPlan.selectedFrameCount}-frame template application`);}catch(error){onError(errorMessage(error));}finally{templateBusy=false;}}
  async function loadQuality(id:string){try{qualityReport=(await api.getQualityReport(id))??undefined;}catch(error){onError(errorMessage(error));}}
  async function analyze(){if(!animationId){onError("Save the animation before analyzing it");return;}try{qualityOpen=true;qualityJob=await api.queueQualityAnalysis(animationId);onNotice("Quality analysis queued");}catch(error){onError(errorMessage(error));}}
  async function ignoreCheck(check:QualityCheck){try{await api.acknowledgeQualityCheck(check.id,true);if(animationId)await loadQuality(animationId);}catch(error){onError(errorMessage(error));}}
  async function repairCheck(check:QualityCheck){if(check.frameIndex!==undefined)activeFrame=check.frameIndex;if(check.repairAction==="remove_duplicate"&&check.frameIndex!==undefined){removeFrame(check.frameIndex);onNotice(`Removed duplicate Frame ${check.frameIndex+1} from the unsaved timeline`);return;}if(animationId&&["auto_align","add_padding","normalize_dimensions"].includes(check.repairAction??"")){repairing=true;try{const repaired=await api.repairAnimationAlignment(animationId);onAnimations(await api.listAnimations(workspaceId,worktreeId));onSelected(repaired);qualityOpen=true;qualityJob=await api.queueQualityAnalysis(repaired.id);onNotice("Created a preserved, aligned animation revision and started re-analysis");}catch(error){onError(errorMessage(error));}finally{repairing=false;}return;}onNotice("Selected the affected frame. Regeneration guidance is available in the warning details.");}
  async function optimizeFrames(){if(!animationId)return;optimizing=true;try{const result=await api.optimizeAnimationFrames(animationId,3);onAnimations(await api.listAnimations(workspaceId,worktreeId));qualityReport=undefined;onSelected(result.animation);qualityOpen=true;qualityJob=await api.queueQualityAnalysis(result.animation.id);onNotice(`${result.summary}. Created a preserved revision and started re-analysis`);}catch(error){onError(errorMessage(error));}finally{optimizing=false;}}
  onMount(()=>{const unlistenPromise=listen<JobEvent>("job-event",async({payload})=>{if(payload.job.kind!=="quality_analysis"||payload.job.id!==qualityJob?.id)return;qualityJob=payload.job;if(payload.job.status==="completed"&&animationId){await loadQuality(animationId);onNotice("Animation quality report completed");}if(payload.job.status==="failed")onError(payload.job.errorMessage??"Quality analysis failed");});return()=>{unlistenPromise.then(unlisten=>unlisten());};});
</script>

<section class="editor">
  <header><div><h1>Animation editor</h1><p>{animations.length} animation{animations.length===1?"":"s"} · {templates.length} template{templates.length===1?"":"s"} · {frames.length} frame{frames.length===1?"":"s"}</p></div><div class="actions"><button type="button" onclick={() => startNewAnimationDraft()}><Plus size={13}/> New</button><button onclick={()=>templateDialog=true} disabled={!animationId||!frames.length}><FileKey2 size={13}/> Save template</button><button onclick={()=>{qualityOpen=true;if(!qualityReport&&!qualityJob)void analyze()}} disabled={!animationId||!frames.length||repairing}><ShieldCheck size={13}/>{repairing?"Repairing…":qualityReport?Math.round(qualityReport.overallScore):"Quality"}</button><button onclick={save} disabled={saving}><Save size={13}/>{saving?"Saving…":"Save"}</button><button class="primary" onclick={exportSheet} disabled={!animationId || !frames.length}><Download size={13}/> Export</button></div></header>
  <div class="body" class:with-quality={qualityOpen}>
    <aside class="animation-list"><div class="label">ANIMATIONS</div>{#each animations as animation}<button class:active={animation.id===animationId} onclick={()=>selectAnimation(animation)}><Clapperboard size={13}/><span>{animation.name}</span><small>{animation.frames.length}</small></button>{/each}{#if !animations.length}<p>No saved animations</p>{/if}<div class="label templates-label">MOTION TEMPLATES</div>{#each templates as template}<button class="template" onclick={()=>applyingTemplate=template}><FileKey2 size={13}/><span>{template.name}</span><small>{template.frameMode==="auto"?`${template.minFrames}–${template.maxFrames}`:template.preferredFrames}</small></button>{/each}{#if !templates.length}<p>Save an animation as reusable motion</p>{/if}</aside>
    <div class="workspace">
      <div class="properties"><label>Name<input bind:value={name}/></label><label>FPS<input type="number" min="1" max="60" bind:value={fps}/></label><label class="check"><input type="checkbox" bind:checked={looping}/><Repeat2 size={12}/> Loop</label><label>Preview<select bind:value={scale}><option value={1}>1×</option><option value={2}>2×</option><option value={3}>3×</option><option value={4}>4×</option></select></label><label class="check"><input type="checkbox" bind:checked={onionSkin}/> Onion skin</label>{#if onionSkin}<label class="onion-opacity">Opacity<input aria-label="Onion skin opacity" type="range" min="0.08" max="0.55" step="0.01" bind:value={onionOpacity}/></label>{/if}</div>
      <div class="preview-area">
        <div class="preview-stage">
          {#if currentAsset}
            {#if onionSkin && previousAsset}<img class="onion previous" src={assetUrl(previousAsset.path)} alt="Previous frame onion skin" style={`transform:scale(${scale});opacity:${onionOpacity}`}/>{/if}
            {#if onionSkin && nextAsset}<img class="onion next" src={assetUrl(nextAsset.path)} alt="Next frame onion skin" style={`transform:scale(${scale});opacity:${onionOpacity}`}/>{/if}
            <img class="current" src={assetUrl(currentAsset.path)} alt={currentAsset.name} style={`transform:scale(${scale})`}/>
          {:else}<div class="preview-empty"><Clapperboard size={25}/><span>Drop image assets into the timeline</span></div>{/if}
        </div>
        <div class="playback"><button onclick={()=>activeFrame=0} title="First frame"><SkipBack size={14}/></button><button class="play" onclick={()=>playing=!playing} disabled={!frames.length} title={playing?"Pause animation":"Play animation"}>{#if playing}<Pause size={15}/>{:else}<Play size={15} fill="currentColor"/>{/if}</button><button onclick={()=>activeFrame=Math.min(frames.length-1,activeFrame+1)} title="Next frame"><SkipForward size={14}/></button><span>Frame {frames.length ? activeFrame+1 : 0} / {frames.length}</span></div>
      </div>
      <div class="timeline" role="region" aria-label="Animation timeline" ondragover={(event)=>event.preventDefault()} ondrop={(event)=>drop(event)}>
        <div class="timeline-head"><span>TIMELINE</span><small>Drop assets here · default {Math.round(1000/fps)} ms/frame</small></div>
        <div class="frames">
          {#each frames as frame,index}
            {@const asset=frameAsset(frame)}
            <button class="frame" class:active={index===activeFrame} class:quality-warning={frameSeverity(index)==="warning"} class:quality-error={frameSeverity(index)==="error"} class:quality-good={frameSeverity(index)==="good"} onclick={()=>activeFrame=index} draggable="true" ondragstart={()=>draggedIndex=index} ondragover={(event)=>event.preventDefault()} ondrop={(event)=>{event.stopPropagation();drop(event,index)}}>
              <span class="number">{String(index+1).padStart(2,"0")}{#if frameSeverity(index)}<i class={frameSeverity(index)}></i>{/if}</span><span class="frame-image">{#if asset}<img src={assetUrl(asset.path)} alt={asset.name}/>{:else}<span class="missing">!</span>{/if}</span>
              <span class="duration"><input type="number" min="16" max="5000" value={frame.durationMs ?? Math.round(1000/fps)} onchange={(event)=>{const next=[...frames];next[index]={...frame,durationMs:Number(event.currentTarget.value)};frames=next;}}/> ms</span>
              <span class="frame-actions"><span role="button" tabindex="0" title="Duplicate" onclick={(event)=>{event.stopPropagation();duplicate(index)}} onkeydown={(event)=>{if(event.key==="Enter"){event.stopPropagation();duplicate(index)}}}><Copy size={11}/></span><span role="button" tabindex="0" title="Remove" onclick={(event)=>{event.stopPropagation();removeFrame(index)}} onkeydown={(event)=>{if(event.key==="Enter"){event.stopPropagation();removeFrame(index)}}}><Trash2 size={11}/></span><GripVertical size={11}/></span>
            </button>
          {/each}
          {#if !frames.length}<div class="drop-target">Drop frames from the asset browser or choose below</div>{/if}
        </div>
        <div class="asset-tray"><span>ADD FRAME</span><div>{#each assets as asset}<button onclick={()=>addFrame(asset.id)} title={`Add ${asset.name}`}><img src={assetUrl(asset.path)} alt={asset.name}/></button>{/each}{#if !assets.length}<small>Import assets first</small>{/if}</div></div>
      </div>
    </div>
    {#if qualityOpen}<QualityPanel report={qualityReport} job={qualityJob} {canOptimize} {optimizing} onAnalyze={analyze} onOptimize={optimizeFrames} onFrame={(index)=>activeFrame=index} onIgnore={ignoreCheck} onRepair={repairCheck} onClose={()=>qualityOpen=false}/>{/if}
  </div>
</section>
{#if templateDialog}<TemplateDialog animationName={name} frameCount={frames.length} busy={templateBusy} onCreate={createTemplate} onClose={()=>templateDialog=false}/>{/if}
{#if applyingTemplate}<TemplateApplyDialog template={applyingTemplate} {assets} busy={templateBusy} onApply={prepareTemplate} onClose={()=>applyingTemplate=undefined}/>{/if}

<style>
  .editor{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 13px 0 17px}header h1{font-size:12px;margin:0}header p{font-size:11px;color:var(--faint);margin:3px 0 0}.actions{display:flex;gap:5px}.actions button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:11px;cursor:pointer}.actions button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}button:disabled{opacity:.4;cursor:not-allowed}
  .body{flex:1;min-height:0;display:grid;grid-template-columns:190px minmax(0,1fr)}.body.with-quality{grid-template-columns:190px minmax(0,1fr) 285px}.animation-list{border-right:1px solid var(--border);background:var(--sidebar);padding:12px 8px;overflow:auto}.label{font-size:10px;color:var(--faint);letter-spacing:.13em;font-weight:700;padding:4px 7px 7px}.templates-label{border-top:1px solid var(--border);margin-top:10px;padding-top:14px}.animation-list>button{width:100%;height:29px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:14px minmax(0,1fr) 24px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:12px;cursor:pointer}.animation-list>button.active,.animation-list>button:hover{background:var(--selected);color:var(--text)}.animation-list>button.template :global(svg){color:var(--accent)}.animation-list button span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.animation-list button small{font-size:9px;color:var(--faint);text-align:right}.animation-list p{font-size:10px;line-height:1.45;color:var(--faint);padding:3px 7px}
  .workspace{min-width:0;min-height:0;display:grid;grid-template-rows:46px minmax(220px,1fr) 255px}.properties{border-bottom:1px solid var(--border);display:flex;align-items:center;gap:14px;padding:0 14px}.properties label{font-size:10px;color:var(--faint);display:flex;align-items:center;gap:6px}.properties label:first-child input{width:150px}.properties input[type="number"]{width:48px}.properties input,.properties select{height:25px;box-sizing:border-box;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}.properties .check{color:var(--muted)}.properties .check input{height:auto}.properties select{width:50px}.properties .onion-opacity{gap:4px}.properties .onion-opacity input{width:58px;height:auto;padding:0;accent-color:var(--accent)}
  .preview-area{min-height:0;display:flex;flex-direction:column;align-items:center;justify-content:center;background:var(--bg);overflow:hidden}.preview-stage{position:relative;width:330px;height:230px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border:1px solid var(--border-strong);box-shadow:0 14px 36px #0005}.preview-stage img{position:absolute;max-width:45%;max-height:45%;object-fit:contain;image-rendering:pixelated;transform-origin:center}.preview-stage img.current{z-index:2}.preview-stage img.onion{z-index:1;filter:saturate(.35)}.preview-stage img.onion.previous{mix-blend-mode:screen;filter:sepia(1) saturate(5) hue-rotate(160deg)}.preview-stage img.onion.next{mix-blend-mode:screen;filter:sepia(1) saturate(5) hue-rotate(285deg)}.preview-empty{display:flex;flex-direction:column;gap:10px;align-items:center;color:var(--faint);font-size:11px}.playback{display:flex;align-items:center;gap:5px;margin-top:14px}.playback button{width:27px;height:27px;border:1px solid var(--border-strong);background:var(--surface);color:var(--muted);border-radius:4px;display:grid;place-items:center;cursor:pointer}.playback .play{width:34px;background:var(--text);color:var(--bg)}.playback>span{font-size:11px;color:var(--faint);margin-left:8px}
  .timeline{border-top:1px solid var(--border);background:var(--sidebar);min-width:0;overflow:hidden}.timeline-head{height:30px;display:flex;align-items:center;justify-content:space-between;padding:0 13px}.timeline-head span,.asset-tray>span{font-size:10px;color:var(--faint);font-weight:700;letter-spacing:.12em}.timeline-head small{font-size:10px;color:var(--faint)}.frames{height:143px;display:flex;gap:6px;padding:0 12px 7px;overflow-x:auto}.frame{width:98px;min-width:98px;height:138px;border:1px solid var(--border);background:var(--surface);color:var(--muted);padding:0;border-radius:5px;display:grid;grid-template-rows:20px 72px 22px 20px;cursor:pointer;overflow:hidden}.frame.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.frame.quality-warning:not(.active){border-color:#8f6c36}.frame.quality-error:not(.active){border-color:#94504c}.number{font-size:10px;color:var(--faint);display:flex;align-items:center;padding:0 6px}.number i{width:6px;height:6px;border-radius:50%;margin-left:auto}.number i.good{background:#5ead7b}.number i.warning{background:#c89a4b}.number i.error{background:#cc6863}.frame-image{display:grid;place-items:center;background:var(--preview)}.frame-image img{max-width:86%;max-height:86%;object-fit:contain;image-rendering:pixelated}.missing{color:#c56f6b}.duration{font-size:7px;color:var(--faint);display:flex;align-items:center;justify-content:center;gap:2px}.duration input{width:38px;border:0;border-bottom:1px solid var(--border);background:transparent;color:var(--muted);font:inherit;font-size:10px;text-align:right;outline:0}.frame-actions{border-top:1px solid var(--border);display:flex;align-items:center;justify-content:flex-end;gap:6px;padding:0 5px;color:var(--faint)}.frame-actions span{display:grid;place-items:center}.drop-target{height:136px;min-width:250px;border:1px dashed var(--border-strong);border-radius:5px;display:grid;place-items:center;color:var(--faint);font-size:11px}.asset-tray{height:72px;border-top:1px solid var(--border);padding:8px 12px;box-sizing:border-box;display:flex;gap:13px}.asset-tray>span{padding-top:5px}.asset-tray>div{display:flex;gap:5px;overflow-x:auto}.asset-tray button{width:45px;height:45px;min-width:45px;border:1px solid var(--border);background:var(--preview);border-radius:4px;padding:3px;cursor:pointer}.asset-tray button:hover{border-color:var(--accent)}.asset-tray img{width:100%;height:100%;object-fit:contain;image-rendering:pixelated}.asset-tray small{font-size:10px;color:var(--faint);padding:5px}
</style>
