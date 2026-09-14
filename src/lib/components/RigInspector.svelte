<script lang="ts">
  import { Layers, Plus, Trash2 } from "lucide-svelte";
  import { transformOf } from "$lib/rig-draft";
  import type { RigBone, RigFrame, RigPoint, RigPointKind } from "$lib/types";

  let {
    panelTab, points, bones, frames, selectedPointId, selectedBoneId, frameIndex, selectedPoint, selectedFrame, pointKinds,
    onTab, onSelectPoint, onSelectBone, onSelectFrame, onUpdatePoint, onRemovePoint, onAddBone, onUpdateBone, onRemoveBone,
    onAddFrame, onUpdateFrame, onSetTransform, onAddContact, onUpdateContact, onRemoveContact,
    interpolateBusy = false, onInterpolate, onRenderInBetweens,
  }: {
    panelTab: "points" | "bones" | "frames";
    points: RigPoint[];
    bones: RigBone[];
    frames: RigFrame[];
    selectedPointId?: string;
    selectedBoneId?: string;
    frameIndex: number;
    selectedPoint?: RigPoint;
    selectedFrame?: RigFrame;
    pointKinds: RigPointKind[];
    onTab: (tab: "points" | "bones" | "frames") => void;
    onSelectPoint: (id: string) => void;
    onSelectBone: (id: string) => void;
    onSelectFrame: (index: number) => void;
    onUpdatePoint: (id: string, patch: Partial<RigPoint>) => void;
    onRemovePoint: (id: string) => void;
    onAddBone: () => void;
    onUpdateBone: (id: string, patch: Partial<RigBone>) => void;
    onRemoveBone: (id: string) => void;
    onAddFrame: () => void;
    onUpdateFrame: (index: number, patch: Partial<RigFrame>) => void;
    onSetTransform: (frameIndex: number, boneName: string, patch: Partial<{ rotate: number; dx: number; dy: number }>) => void;
    onAddContact: (frameIndex: number) => void;
    onUpdateContact: (frameIndex: number, index: number, patch: Partial<{ bone: string; x: number; y: number; bend: number }>) => void;
    onRemoveContact: (frameIndex: number, index: number) => void;
    interpolateBusy?: boolean;
    onInterpolate?: (fromIndex: number, toIndex: number, steps: number) => void;
    onRenderInBetweens?: (fromIndex: number, toIndex: number, steps: number) => void;
  } = $props();

  let interpolateFrom = $state(0);
  let interpolateTo = $state(1);
  let interpolateSteps = $state(2);

  $effect(() => {
    if (frames.length < 2) return;
    interpolateFrom = Math.min(frameIndex, frames.length - 2);
    interpolateTo = Math.min(interpolateFrom + 1, frames.length - 1);
  });
</script>

<aside class="panel">
  <nav>
    <button class:active={panelTab === "points"} onclick={() => onTab("points")}>Points</button>
    <button class:active={panelTab === "bones"} onclick={() => onTab("bones")}>Bones</button>
    <button class:active={panelTab === "frames"} onclick={() => onTab("frames")}>Poses</button>
  </nav>
  <div class="panel-body">
    {#if panelTab === "points"}
      {#if selectedPoint}
        <div class="inspector">
          <label>Name<input value={selectedPoint.name} onchange={(event) => onUpdatePoint(selectedPoint.id, { name: event.currentTarget.value })}/></label>
          <label>Kind<select value={selectedPoint.kind} onchange={(event) => onUpdatePoint(selectedPoint.id, { kind: event.currentTarget.value as RigPointKind })}>{#each pointKinds as kind}<option value={kind}>{kind}</option>{/each}</select></label>
          <div class="pair"><label>X<input type="number" step="0.5" value={selectedPoint.x} onchange={(event) => onUpdatePoint(selectedPoint.id, { x: Number(event.currentTarget.value) })}/></label>
          <label>Y<input type="number" step="0.5" value={selectedPoint.y} onchange={(event) => onUpdatePoint(selectedPoint.id, { y: Number(event.currentTarget.value) })}/></label></div>
          <p class="meta">{selectedPoint.source} suggestion · {Math.round(selectedPoint.confidence * 100)}% confidence{selectedPoint.note ? ` · ${selectedPoint.note}` : ""}</p>
          <button class="danger" onclick={() => onRemovePoint(selectedPoint.id)}><Trash2 size={11}/> Remove point</button>
        </div>
      {/if}
      <div class="item-list">
        {#each points as point (point.id)}
          <button class:active={point.id === selectedPointId} onclick={() => onSelectPoint(point.id)}>
            <i class="kind" class:contact={point.kind === "contact"}></i><span>{point.name}</span><small>{point.kind}</small><small class="coords">{Math.round(point.x)},{Math.round(point.y)}</small>
          </button>
        {/each}
        {#if !points.length}<p class="hint">Click the canvas or use Auto place / Ask AI.</p>{/if}
      </div>
    {:else if panelTab === "bones"}
      <button class="add-row" onclick={onAddBone}><Plus size={12}/> Add bone</button>
      <div class="item-list tall">
        {#each bones as bone (bone.id)}
          <div class="bone-card" class:active={bone.id === selectedBoneId}>
            <header role="button" tabindex="0" onclick={() => onSelectBone(bone.id)} onkeydown={(event) => { if (event.key === "Enter") onSelectBone(bone.id); }}><Layers size={12}/><input value={bone.name} onchange={(event) => onUpdateBone(bone.id, { name: event.currentTarget.value })}/><small>z {bone.z}</small></header>
            <div class="bone-grid">
              <label>Start<select value={bone.startPoint} onchange={(event) => onUpdateBone(bone.id, { startPoint: event.currentTarget.value })}>{#each points as point}<option value={point.name}>{point.name}</option>{/each}</select></label>
              <label>End<select value={bone.endPoint} onchange={(event) => onUpdateBone(bone.id, { endPoint: event.currentTarget.value })}>{#each points as point}<option value={point.name}>{point.name}</option>{/each}</select></label>
              <label>Radius<input type="number" min="0.5" max="64" step="0.5" value={bone.radius} onchange={(event) => onUpdateBone(bone.id, { radius: Number(event.currentTarget.value) })}/></label>
              <label>Layer<input type="number" step="1" value={bone.z} onchange={(event) => onUpdateBone(bone.id, { z: Number(event.currentTarget.value) })}/></label>
              <label class="wide">Parent<select value={bone.parent ?? ""} onchange={(event) => onUpdateBone(bone.id, { parent: event.currentTarget.value })}><option value="">none</option>{#each bones.filter(candidate => candidate.id !== bone.id) as candidate}<option value={candidate.name}>{candidate.name}</option>{/each}</select></label>
              <button class="danger icon" onclick={() => onRemoveBone(bone.id)} title="Remove bone"><Trash2 size={11}/></button>
            </div>
          </div>
        {/each}
        {#if !bones.length}<p class="hint">Bones are capsules between two points. The renderer claims every pixel inside a capsule, and leftovers go to the nearest bone.</p>{/if}
      </div>
    {:else}
      <div class="frame-strip">
        {#each frames as frame, index (index)}
          <button class:active={index === frameIndex} onclick={() => onSelectFrame(index)}>
            <strong>{String(index + 1).padStart(2, "0")}</strong>
            <span>{frame.phase || (frame.hold ? "hold" : "pose")}</span>
          </button>
        {/each}
        <button class="add" onclick={onAddFrame} title="Add frame"><Plus size={12}/></button>
      </div>
      {#if selectedFrame}
        <div class="inspector">
          <div class="pair">
            <label>Phase<input value={selectedFrame.phase ?? ""} placeholder="contact" onchange={(event) => onUpdateFrame(frameIndex, { phase: event.currentTarget.value || undefined })}/></label>
            <label class="checkbox-row"><input type="checkbox" checked={selectedFrame.hold} onchange={(event) => onUpdateFrame(frameIndex, { hold: event.currentTarget.checked })}/> Hold</label>
          </div>
          <div class="pair">
            <label>Root dx<input type="number" step="1" value={selectedFrame.rootDx} onchange={(event) => onUpdateFrame(frameIndex, { rootDx: Number(event.currentTarget.value) })}/></label>
            <label>Root dy<input type="number" step="1" value={selectedFrame.rootDy} onchange={(event) => onUpdateFrame(frameIndex, { rootDy: Number(event.currentTarget.value) })}/></label>
          </div>
        </div>
        <div class="transform-list">
          <header><span>BONE</span><span>ROTATE°</span><span>DX</span><span>DY</span></header>
          {#each bones as bone (bone.id)}
            {@const transform = transformOf(selectedFrame, bone.name)}
            <div class="transform-row">
              <span title={bone.name}>{bone.name}</span>
              <input type="number" step="1" value={transform?.rotate ?? 0} onchange={(event) => onSetTransform(frameIndex, bone.name, { rotate: Number(event.currentTarget.value) })}/>
              <input type="number" step="1" value={transform?.dx ?? 0} onchange={(event) => onSetTransform(frameIndex, bone.name, { dx: Number(event.currentTarget.value) })}/>
              <input type="number" step="1" value={transform?.dy ?? 0} onchange={(event) => onSetTransform(frameIndex, bone.name, { dy: Number(event.currentTarget.value) })}/>
            </div>
          {/each}
        </div>
        <div class="contacts">
          <header><span>PLANTED CONTACTS</span><button onclick={() => onAddContact(frameIndex)} disabled={!bones.length}><Plus size={11}/> Pin</button></header>
          {#each selectedFrame.contacts as contact, index}
            <div class="contact-row">
              <select value={contact.bone} onchange={(event) => onUpdateContact(frameIndex, index, { bone: event.currentTarget.value })}>{#each bones as bone}<option value={bone.name}>{bone.name}</option>{/each}</select>
              <input type="number" step="0.5" value={contact.x} title="Target x" onchange={(event) => onUpdateContact(frameIndex, index, { x: Number(event.currentTarget.value) })}/>
              <input type="number" step="0.5" value={contact.y} title="Target y" onchange={(event) => onUpdateContact(frameIndex, index, { y: Number(event.currentTarget.value) })}/>
              <select value={contact.bend >= 0 ? "1" : "-1"} title="Bend direction" onchange={(event) => onUpdateContact(frameIndex, index, { bend: Number(event.currentTarget.value) })}><option value="1">↷</option><option value="-1">↶</option></select>
              <button class="danger icon" onclick={() => onRemoveContact(frameIndex, index)} title="Remove contact"><Trash2 size={11}/></button>
            </div>
          {/each}
          {#if !selectedFrame.contacts.length}<p class="hint">Contacts plant a bone's end point in place with two-bone IK — feet stop sliding.</p>{/if}
        </div>
        {#if frames.length >= 2 && (onInterpolate || onRenderInBetweens)}
          <div class="interpolate">
            <header><span>INTERPOLATE A→B</span></header>
            <div class="pair">
              <label>From<select bind:value={interpolateFrom}>{#each frames as _, index}<option value={index}>{index + 1}</option>{/each}</select></label>
              <label>To<select bind:value={interpolateTo}>{#each frames as _, index}<option value={index}>{index + 1}</option>{/each}</select></label>
            </div>
            <label>Steps<input type="number" min="1" max="16" bind:value={interpolateSteps}/></label>
            {#if onInterpolate}
              <button class="interp-btn" disabled={interpolateBusy || interpolateFrom >= interpolateTo} onclick={() => onInterpolate?.(interpolateFrom, interpolateTo, Number(interpolateSteps))}>
                {interpolateBusy ? "Interpolating…" : `Insert ${Math.max(0, Number(interpolateSteps) || 0)} pose frame(s)`}
              </button>
            {/if}
            {#if onRenderInBetweens}
              <button class="interp-btn render" disabled={interpolateBusy || interpolateFrom >= interpolateTo} onclick={() => onRenderInBetweens?.(interpolateFrom, interpolateTo, Number(interpolateSteps))}>
                {interpolateBusy ? "Rendering…" : "Insert rendered in-betweens"}
              </button>
            {/if}
          </div>
        {/if}
      {:else}
        <p class="hint">Add pose frames to keyframe rotations per bone. Preview or render at any time.</p>
      {/if}
    {/if}
  </div>
</aside>

<style>
  .panel{height:100%;border-left:1px solid var(--border);background:var(--sidebar);display:flex;flex-direction:column;min-height:0}.panel nav{display:flex;border-bottom:1px solid var(--border);height:33px;flex:0 0 auto}.panel nav button{flex:1;border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;letter-spacing:.08em;cursor:pointer;border-bottom:2px solid transparent}.panel nav button.active{color:var(--text);border-bottom-color:var(--accent)}.panel-body{flex:1;min-height:0;overflow:auto;padding:10px}
  .inspector{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:9px;margin-bottom:10px}.inspector label{display:block;font-size:9px;color:var(--faint);letter-spacing:.06em;margin-top:6px}.inspector label:first-child{margin-top:0}.inspector input,.inspector select{display:block;width:100%;height:24px;box-sizing:border-box;margin-top:3px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 5px;outline:0}.inspector .pair{display:grid;grid-template-columns:1fr 1fr;gap:6px}.inspector .meta{font-size:9px;color:var(--faint);margin:7px 0 0}.inspector button.danger{margin-top:8px;height:24px;border:1px solid var(--border);border-radius:4px;background:transparent;color:#cc7a74;font:inherit;font-size:10px;display:flex;align-items:center;gap:5px;padding:0 7px;cursor:pointer}
  .item-list{display:flex;flex-direction:column;gap:3px}.item-list.tall{gap:6px}.item-list>button{height:27px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:10px minmax(0,1fr) 44px 52px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:11px;cursor:pointer}.item-list>button.active,.item-list>button:hover{background:var(--selected);color:var(--text)}.item-list>button i.kind{width:7px;height:7px;border-radius:50%;background:#69d2c5}.item-list>button i.kind.contact{background:#e0a458}.item-list small{font-size:9px;color:var(--faint)}.coords{text-align:right}.hint{font-size:10px;line-height:1.5;color:var(--faint);padding:4px 2px}
  .add-row{height:26px;border:1px dashed var(--border-strong);border-radius:5px;background:transparent;color:var(--muted);font:inherit;font-size:10px;display:flex;align-items:center;justify-content:center;gap:5px;width:100%;cursor:pointer;margin-bottom:8px}
  .bone-card{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:7px}.bone-card.active{border-color:var(--accent)}.bone-card header{display:flex;align-items:center;gap:6px;height:24px}.bone-card header :global(svg){color:var(--faint);flex:0 0 auto}.bone-card header input{flex:1;min-width:0;height:23px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 5px;outline:0}.bone-card header small{font-size:9px;color:var(--faint)}.bone-grid{display:grid;grid-template-columns:1fr 1fr 1fr 26px;gap:5px;margin-top:7px;align-items:end}.bone-grid label{font-size:9px;color:var(--faint);display:block}.bone-grid input,.bone-grid select{display:block;width:100%;height:22px;box-sizing:border-box;margin-top:3px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:0 4px;outline:0}.bone-grid .wide{grid-column:span 3}.bone-grid button.icon,.contact-row button.icon{height:22px;width:26px;border:1px solid var(--border);border-radius:4px;background:transparent;color:#cc7a74;display:grid;place-items:center;cursor:pointer}
  .frame-strip{display:flex;gap:4px;flex-wrap:wrap;margin-bottom:10px}.frame-strip>button{min-width:46px;height:38px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:1px;font:inherit;cursor:pointer}.frame-strip>button strong{font-size:11px}.frame-strip>button span{font-size:8px;max-width:56px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.frame-strip>button.active{border-color:var(--accent);color:var(--text)}.frame-strip>button.add{justify-content:center}
  .transform-list{border:1px solid var(--border);border-radius:6px;overflow:hidden;margin-top:10px}.transform-list header,.transform-row{display:grid;grid-template-columns:minmax(0,1fr) 48px 40px 40px;gap:4px;padding:4px 6px;align-items:center}.transform-list header{background:var(--surface);font-size:8px;letter-spacing:.08em;color:var(--faint)}.transform-row{border-top:1px solid var(--border);font-size:10px;color:var(--muted)}.transform-row span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.transform-row input{width:100%;height:21px;box-sizing:border-box;background:var(--bg);border:1px solid var(--border);border-radius:3px;color:var(--text);font:inherit;font-size:10px;padding:0 3px;outline:0}
  .contacts{margin-top:12px}.contacts header{display:flex;align-items:center;justify-content:space-between;font-size:9px;letter-spacing:.1em;color:var(--faint);margin-bottom:6px}.contacts header button{height:22px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--muted);font:inherit;font-size:9px;display:flex;align-items:center;gap:4px;padding:0 6px;cursor:pointer}.contact-row{display:grid;grid-template-columns:minmax(0,1fr) 42px 42px 32px 26px;gap:4px;margin-bottom:4px}.contact-row select,.contact-row input{height:22px;box-sizing:border-box;background:var(--bg);border:1px solid var(--border);border-radius:3px;color:var(--text);font:inherit;font-size:10px;padding:0 3px;outline:0}
  .pair .checkbox-row{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:11px}.checkbox-row input{height:auto;width:auto}
  .interpolate{margin-top:12px;border:1px solid var(--border);border-radius:6px;padding:8px}.interpolate header{font-size:9px;letter-spacing:.1em;color:var(--faint);margin-bottom:6px}.interpolate label{display:block;font-size:9px;color:var(--faint);margin-top:6px}.interpolate input,.interpolate select{display:block;width:100%;height:22px;box-sizing:border-box;margin-top:3px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:0 4px;outline:0}.interp-btn{width:100%;height:26px;margin-top:8px;border:1px dashed var(--accent);border-radius:5px;background:transparent;color:var(--accent);font:inherit;font-size:10px;cursor:pointer}.interp-btn:hover{background:var(--accent-dim)}.interp-btn:disabled{opacity:.45;cursor:not-allowed}
  button:disabled{opacity:.4;cursor:not-allowed}
</style>
