<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Bone, Check, Clapperboard, Plus, Save, Sparkles, Trash2, Wand2 } from "lucide-svelte";
  import { api } from "$lib/api";
  import RigInspector from "$lib/components/RigInspector.svelte";
  import RigStage from "$lib/components/RigStage.svelte";
  import {
    addBone as draftAddBone, addContact as draftAddContact, addFrame as draftAddFrame, addPoint as draftAddPoint,
    cloneRigDraft, cloneSuggestionDraft, currentInput, nextPreviewTick, removeBone as draftRemoveBone,
    removeContact as draftRemoveContact, removePoint as draftRemovePoint,
    setTransform as draftSetTransform, updateBone as draftUpdateBone, updateContact as draftUpdateContact,
    updateFrame as draftUpdateFrame, updatePoint as draftUpdatePoint,
  } from "$lib/rig-draft";
  import { errorMessage, RIG_MORPHOLOGIES, type Animation, type Asset, type JobEvent, type ProviderStatus, type Rig, type RigBone, type RigFitReport, type RigFrame, type RigMorphology, type RigPoint, type RigPointKind, type RigSuggestion } from "$lib/types";

  let { workspaceId, worktreeId, assets, rigs, providers, selectedRigId, linkedAnimationNameForRig, linkedAnimationIdForRig, initialAssetId, onRigs, onSelected, onRendered, onPolish, onError, onNotice }: {
    workspaceId: string; worktreeId?: string; assets: Asset[]; rigs: Rig[]; providers: ProviderStatus[]; selectedRigId?: string;
    linkedAnimationNameForRig?: (rigId: string) => string | undefined; linkedAnimationIdForRig?: (rigId: string) => string | undefined; initialAssetId?: string; onRigs: (rigs: Rig[]) => void; onSelected: (id?: string) => void;
    onRendered: (animation: Animation, assetIds: string[]) => void;
    onPolish: (animation: Animation, assetIds: string[]) => void; onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();

  let loadedRigId = $state<string | undefined>();
  let rigId = $state<string | undefined>();
  let name = $state("New rig");
  let morphology = $state<RigMorphology>("biped");
  let fps = $state(8);
  let looping = $state(true);
  let masterAssetId = $state<string | undefined>();
  let points = $state<RigPoint[]>([]);
  let bones = $state<RigBone[]>([]);
  let frames = $state<RigFrame[]>([]);
  let selectedPointId = $state<string | undefined>();
  let selectedBoneId = $state<string | undefined>();
  let frameIndex = $state(0);
  let panelTab = $state<"points" | "bones" | "frames">("points");
  let scale = $state(3);
  let saving = $state(false);
  let rendering = $state(false);
  let warnings = $state<string[]>([]);
  let suggestion = $state<RigSuggestion>();
  let suggesting = $state(false);
  let aiBusy = $state(false);
  let aiMotion = $state("");
  let aiProviderId = $state<string | undefined>();
  let previewPaths = $state<string[]>([]);
  let previewIndex = $state(0);
  let playing = $state(false);
  let previewBusy = $state(false);
  let idCounter = 0;
  const nextId = (prefix: string) => `${prefix}_${Date.now().toString(36)}_${++idCounter}`;
  let fit = $state<RigFitReport>();
  let fitBusy = $state(false);
  let interpolateBusy = $state(false);
  let pendingInterpolateJob = $state<{ jobId: string; animationId: string; fromIndex: number; insertedCount: number } | undefined>();

  async function reloadInterpolatedAnimation(animationId: string, assetIds: string[], fromIndex: number, insertedCount: number) {
    const animations = await api.listAnimations(workspaceId, worktreeId);
    const animation = animations.find(item => item.id === animationId);
    if (!animation) {
      onError("Animazione collegata non trovata dopo l'inserimento degli in-between");
      return;
    }
    const linkedIds = assetIds.length
      ? assetIds
      : animation.frames.slice(fromIndex + 1, fromIndex + 1 + insertedCount).map(frame => frame.assetId);
    await refreshRigs();
    onRendered(animation, linkedIds);
  }

  const masterAsset = $derived(assets.find(asset => asset.id === masterAssetId));
  const agentProviders = $derived(providers.filter(provider => provider.kind === "agent" && ["ready", "detected"].includes(provider.status)));
  const selectedPoint = $derived(points.find(point => point.id === selectedPointId));
  const selectedFrame = $derived(frames[frameIndex]);
  const showingPreview = $derived(previewPaths.length > 0 && previewIndex < previewPaths.length);
  const stageImage = $derived(showingPreview ? previewPaths[previewIndex] : masterAsset?.path);
  const pointKinds: RigPointKind[] = ["joint", "anchor", "contact", "pivot"];

  $effect(() => {
    if (initialAssetId && !selectedRigId) {
      if (masterAssetId !== initialAssetId) {
        resetDraftFields();
        masterAssetId = initialAssetId;
      }
      return;
    }
    const target = rigs.find(rig => rig.id === (selectedRigId ?? rigs[0]?.id));
    if (target && target.id !== loadedRigId) loadRig(target);
  });
  $effect(() => {
    const id = masterAsset?.id;
    if (!id) { fit = undefined; return; }
    void runFit(id);
  });
  $effect(() => {
    if (!playing || !previewPaths.length) return;
    const timer = window.setTimeout(() => {
      const tick = nextPreviewTick(previewIndex, previewPaths.length, looping);
      previewIndex = tick.previewIndex; playing = tick.playing;
    }, 1000 / fps);
    return () => window.clearTimeout(timer);
  });

  function draftSnapshot() {
    return { id: rigId, workspaceId, worktreeId, assetId: masterAssetId, name, morphology, fps: Number(fps), looping, points, bones, frames };
  }
  function loadRig(rig: Rig) {
    const draft = cloneRigDraft(rig);
    loadedRigId = draft.loadedRigId; rigId = draft.rigId; name = draft.name; morphology = draft.morphology; fps = draft.fps; looping = draft.looping;
    masterAssetId = draft.masterAssetId; points = draft.points; bones = draft.bones; frames = draft.frames;
    frameIndex = 0; selectedPointId = undefined; selectedBoneId = undefined; previewPaths = []; playing = false; suggestion = undefined; warnings = [];
  }
  function resetDraftFields() {
    loadedRigId = undefined; rigId = undefined; name = "New rig"; morphology = "biped"; fps = 8; looping = true;
    masterAssetId = undefined; points = []; bones = []; frames = []; frameIndex = 0; selectedPointId = undefined; selectedBoneId = undefined;
    previewPaths = []; playing = false; suggestion = undefined; warnings = []; fit = undefined;
  }
  function startNewDraft() {
    resetDraftFields();
    onSelected(undefined);
  }
  async function refreshRigs(select?: string) { onRigs(await api.listRigs(workspaceId, worktreeId)); if (select) onSelected(select); }
  function invalidatePreview() { if (previewPaths.length) { previewPaths = []; playing = false; previewIndex = 0; } }

  function addPointAt(x: number, y: number) {
    const result = draftAddPoint(points, x, y, nextId);
    points = result.points; selectedPointId = result.point.id; invalidatePreview();
  }
  function updatePoint(id: string, patch: Partial<RigPoint>) { points = draftUpdatePoint(points, id, patch); invalidatePreview(); }
  function removePoint(id: string) {
    const result = draftRemovePoint(points, bones, id);
    points = result.points; bones = result.bones;
    if (selectedPointId === id) selectedPointId = undefined;
    invalidatePreview();
  }
  function addBone() {
    const result = draftAddBone(points, bones, nextId);
    if (!result) { onError("Add at least two points before creating a bone"); return; }
    bones = result.bones; selectedBoneId = result.bone.id; panelTab = "bones"; invalidatePreview();
  }
  function updateBone(id: string, patch: Partial<RigBone>) { bones = draftUpdateBone(bones, id, patch); invalidatePreview(); }
  function removeBone(id: string) {
    const result = draftRemoveBone(bones, frames, id);
    bones = result.bones; frames = result.frames;
    if (selectedBoneId === id) selectedBoneId = undefined;
    invalidatePreview();
  }
  function addFrame() { frames = draftAddFrame(frames); frameIndex = frames.length - 1; invalidatePreview(); }
  function updateFrame(index: number, patch: Partial<RigFrame>) { frames = draftUpdateFrame(frames, index, patch); invalidatePreview(); }
  function setTransform(frameIndexValue: number, boneName: string, patch: Partial<{ rotate: number; dx: number; dy: number }>) {
    frames = draftSetTransform(frames, frameIndexValue, boneName, patch); invalidatePreview();
  }
  function addContact(frameIndexValue: number) { frames = draftAddContact(frames, frameIndexValue, bones); invalidatePreview(); }
  function updateContact(frameIndexValue: number, index: number, patch: Partial<{ bone: string; x: number; y: number; bend: number }>) {
    frames = draftUpdateContact(frames, frameIndexValue, index, patch); invalidatePreview();
  }
  function removeContact(frameIndexValue: number, index: number) { frames = draftRemoveContact(frames, frameIndexValue, index); invalidatePreview(); }

  async function runFit(assetId: string) {
    fitBusy = true;
    try { fit = await api.analyzeRigFit(assetId); if (fit.detections[0]) morphology = fit.detections[0].morphology; }
    catch (error) { fit = undefined; onError(errorMessage(error)); } finally { fitBusy = false; }
  }
  function applyBestFit() {
    if (!fit) return;
    suggestion = fit.recommended;
    onNotice(`Best rig: ${fit.recommended.morphology} — drag the dashed points, then apply`);
  }
  function useMorphologyTemplate(value: RigMorphology) { morphology = value; void autoSuggest(); }

  async function autoSuggest() {
    if (!masterAsset) { onError("Choose a source sprite first"); return; }
    suggesting = true;
    try { suggestion = await api.suggestRigPoints(masterAsset.id, morphology); onNotice(`Template placed ${suggestion.points.length} points — drag them onto the anatomy, then apply`); }
    catch (error) { onError(errorMessage(error)); } finally { suggesting = false; }
  }
  async function aiSuggest() {
    if (!masterAsset) { onError("Choose a source sprite first"); return; }
    const providerId = aiProviderId ?? agentProviders[0]?.id;
    if (!providerId) { onError("No signed-in agent provider is available. Check Settings."); return; }
    aiBusy = true;
    try {
      suggestion = await api.aiSuggestRigPoints({ assetId: masterAsset.id, morphology, motion: aiMotion.trim() || undefined, providerId });
      onNotice(`${providerId} suggested ${suggestion.points.length} points${suggestion.frames.length ? ` and ${suggestion.frames.length} pose frames` : ""} — review, then apply`);
    } catch (error) { onError(errorMessage(error)); } finally { aiBusy = false; }
  }
  function applySuggestion() {
    if (!suggestion) return;
    const draft = cloneSuggestionDraft(suggestion);
    morphology = draft.morphology; points = draft.points; bones = draft.bones;
    if (draft.frames.length) frames = draft.frames;
    suggestion = undefined; invalidatePreview(); onNotice("Suggestion applied to the canvas");
  }
  function dismissSuggestion() { suggestion = undefined; }

  async function preview() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before previewing"); return; }
    previewBusy = true;
    try { previewPaths = await api.renderRigPreview(currentInput(draftSnapshot())); previewIndex = 0; onNotice(`Rendered ${previewPaths.length} preview frame${previewPaths.length === 1 ? "" : "s"}`); }
    catch (error) { onError(errorMessage(error)); } finally { previewBusy = false; }
  }
  async function save() {
    saving = true;
    try { const rig = await api.saveRig(currentInput(draftSnapshot())); loadedRigId = rig.id; rigId = rig.id; await refreshRigs(rig.id); onNotice("Rig saved"); }
    catch (error) { onError(errorMessage(error)); } finally { saving = false; }
  }
  async function removeRig() {
    if (!rigId) return;
    try { await api.deleteRig(rigId); await refreshRigs(); startNewDraft(); onNotice("Rig deleted"); }
    catch (error) { onError(errorMessage(error)); }
  }
  async function renderAnimation() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before rendering"); return; }
    rendering = true;
    try { warnings = await api.validateRigSpec(currentInput(draftSnapshot())); const result = await api.renderRigAnimation(currentInput(draftSnapshot())); await refreshRigs(); onRendered(result.animation, result.assetIds); }
    catch (error) { onError(errorMessage(error)); } finally { rendering = false; }
  }
  async function renderAnimationInBetweens(fromIndex: number, toIndex: number, steps: number) {
    const animationId = rigId ? linkedAnimationIdForRig?.(rigId) : undefined;
    if (!animationId) {
      onError("Collega il rig a una animazione prima di inserire in-between renderizzati");
      return;
    }
    interpolateBusy = true;
    try {
      const result = await api.interpolateRigAnimationFrames({
        rig: currentInput(draftSnapshot()),
        fromIndex,
        toIndex,
        steps,
        animationId,
        workspaceId,
        worktreeId,
      });
      if (result.jobId) {
        pendingInterpolateJob = {
          jobId: result.jobId,
          animationId,
          fromIndex,
          insertedCount: steps,
        };
        onNotice(`In-between render accodati (job ${result.jobId})`);
      } else {
        await reloadInterpolatedAnimation(animationId, result.assetIds, fromIndex, result.insertedCount);
      }
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      interpolateBusy = false;
    }
  }
  onMount(() => {
    const unlistenPromise = listen<JobEvent>("job-event", async ({ payload }) => {
      const pending = pendingInterpolateJob;
      if (!pending || payload.job.kind !== "rig_interpolate_render" || payload.job.id !== pending.jobId) return;
      if (payload.job.status === "completed") {
        pendingInterpolateJob = undefined;
        await reloadInterpolatedAnimation(
          pending.animationId,
          [],
          pending.fromIndex,
          pending.insertedCount,
        );
      }
      if (payload.job.status === "failed") {
        pendingInterpolateJob = undefined;
        onError(payload.job.errorMessage ?? "Rendered in-between job failed");
      }
      if (payload.job.status === "cancelled") {
        pendingInterpolateJob = undefined;
        onNotice("Rendered in-between job cancelled");
      }
    });
    return () => {
      void unlistenPromise.then(unlisten => unlisten());
    };
  });

  async function interpolateKeyframes(fromIndex: number, toIndex: number, steps: number) {
    if (!masterAsset || fromIndex >= toIndex) {
      onError("Choose two pose frames where A comes before B");
      return;
    }
    interpolateBusy = true;
    try {
      const result = await api.interpolateRigFrames({
        rig: currentInput(draftSnapshot()),
        fromIndex,
        toIndex,
        steps,
      });
      frames = result.frames;
      frameIndex = fromIndex + 1;
      previewPaths = result.previewPaths;
      previewIndex = 0;
      onNotice(`Inserted ${result.insertedCount} interpolated pose frame(s) between ${fromIndex + 1} and ${toIndex + 1}`);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      interpolateBusy = false;
    }
  }

  async function polishWithAi() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before polishing"); return; }
    rendering = true;
    try { warnings = await api.validateRigSpec(currentInput(draftSnapshot())); const result = await api.renderRigAnimation(currentInput(draftSnapshot())); await refreshRigs(); onPolish(result.animation, result.assetIds); }
    catch (error) { onError(errorMessage(error)); } finally { rendering = false; }
  }
</script>

<section class="rig-editor">
  <header>
    <div><h1>Rig editor</h1><p>{rigs.length} rig{rigs.length === 1 ? "" : "s"} · {points.length} points · {bones.length} bones · {frames.length} pose frame{frames.length === 1 ? "" : "s"}{#if rigId && linkedAnimationNameForRig?.(rigId)} · used by {linkedAnimationNameForRig(rigId)}{/if}</p></div>
    <div class="actions">
      <button onclick={() => startNewDraft()}><Plus size={13}/> New</button>
      <button onclick={save} disabled={saving}><Save size={13}/>{saving ? "Saving…" : "Save"}</button>
      {#if rigId}<button class="danger" onclick={removeRig}><Trash2 size={13}/></button>{/if}
      <button onclick={polishWithAi} disabled={rendering || !masterAsset || !bones.length} title="Render the rig deterministically, then let the AI polish detail without swapping limb identity"><Sparkles size={13}/>{rendering ? "Working…" : "Polish with AI"}</button>
      <button class="primary" onclick={renderAnimation} disabled={rendering || !masterAsset || !bones.length}><Clapperboard size={13}/>{rendering ? "Rendering…" : "Render animation"}</button>
    </div>
  </header>
  <div class="body">
    <aside class="rig-list">
      <div class="label">RIGS</div>
      {#each rigs as rig}<button class:active={rig.id === rigId} onclick={() => onSelected(rig.id)}><Bone size={13}/><span>{rig.name}</span><small>{linkedAnimationNameForRig?.(rig.id) ? "anim" : rig.bones.length}</small></button>{/each}
      {#if !rigs.length}<p>Save a rig to keep its points, bones, and poses.</p>{/if}
      <div class="label spacing">RIG BASICS</div>
      <label class="field">Name<input bind:value={name}/></label>
      <label class="field">Morphology<select bind:value={morphology}>{#each RIG_MORPHOLOGIES as entry}<option value={entry.id}>{entry.label}</option>{/each}</select></label>
      <label class="field half-row">FPS<input type="number" min="1" max="60" bind:value={fps}/></label>
      <label class="field checkbox-row"><input type="checkbox" bind:checked={looping}/> Loop playback</label>
      <label class="field">Source sprite<select bind:value={masterAssetId} placeholder="Choose a master">
        <option value="" selected={!masterAssetId}>Choose a source sprite</option>
        {#each assets as asset}<option value={asset.id}>{asset.name} · {asset.width}×{asset.height}</option>{/each}
      </select></label>
      {#if masterAsset}
        <div class="label spacing">RIG CHECK</div>
        {#if fitBusy}<p class="hint">Profiling the silhouette…</p>
        {:else if fit}
          <div class="fit-list">
            {#each fit.detections.slice(0, 3) as detection, rank (detection.morphology)}
              <button class="fit-row" class:best={rank === 0} title={detection.reasoning} onclick={() => useMorphologyTemplate(detection.morphology)}>
                <span class="fit-name">{detection.morphology}</span>
                <span class="fit-bar"><i style={`width:${Math.max(4, Math.round(detection.confidence * 100))}%`}></i></span>
                <small>{Math.round(detection.confidence * 100)}%</small>
              </button>
            {/each}
          </div>
          <p class="hint">Capsules cover {Math.round(fit.capsuleFit * 100)}% of the silhouette directly.</p>
          {#each fit.warnings as warning (warning)}<p class="fit-warning">{warning}</p>{/each}
          <button class="apply-best" onclick={applyBestFit} disabled={Boolean(suggestion)}><Check size={11}/> Apply {fit.detections[0].morphology} template</button>
        {:else}
          <button class="apply-best" onclick={() => masterAsset && void runFit(masterAsset.id)}><Wand2 size={11}/> Check rig fit</button>
        {/if}
      {/if}
      {#if warnings.length}<div class="warnings">{#each warnings as warning}<p title={warning}>{warning}</p>{/each}</div>{/if}
    </aside>
    <RigStage {masterAsset} {bones} {points} {suggestion} {selectedPointId} {scale} {stageImage} {playing} {previewPaths} {previewIndex} {previewBusy} {rendering} {suggesting} {aiBusy} {agentProviders} {aiProviderId} {aiMotion} onAddPoint={addPointAt} onUpdatePoint={updatePoint} onSelectPoint={(id) => selectedPointId = id} onSuggestion={(value) => suggestion = value} onScale={(value) => scale = value} onPreview={preview} onTogglePlay={() => playing = !playing} onAutoSuggest={autoSuggest} onAiSuggest={aiSuggest} onAiProvider={(id) => aiProviderId = id} onAiMotion={(value) => aiMotion = value} onApplySuggestion={applySuggestion} onDismissSuggestion={dismissSuggestion}/>
    <RigInspector {panelTab} {points} {bones} {frames} {selectedPointId} {selectedBoneId} {frameIndex} {selectedPoint} {selectedFrame} {pointKinds} {interpolateBusy} onTab={(tab) => panelTab = tab} onSelectPoint={(id) => selectedPointId = id} onSelectBone={(id) => selectedBoneId = id} onSelectFrame={(index) => frameIndex = index} onUpdatePoint={updatePoint} onRemovePoint={removePoint} onAddBone={addBone} onUpdateBone={updateBone} onRemoveBone={removeBone} onAddFrame={addFrame} onUpdateFrame={updateFrame} onSetTransform={setTransform} onAddContact={addContact} onUpdateContact={updateContact} onRemoveContact={removeContact} onInterpolate={interpolateKeyframes} onRenderInBetweens={renderAnimationInBetweens}/>
  </div>
</section>

<style>
  .rig-editor{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 13px 0 17px}header h1{font-size:12px;margin:0}header p{font-size:11px;color:var(--faint);margin:3px 0 0}.actions{display:flex;gap:5px}.actions button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:11px;cursor:pointer}.actions button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}.actions button.danger{color:#cc7a74}button:disabled{opacity:.4;cursor:not-allowed}
  .body{flex:1;min-height:0;display:grid;grid-template-columns:212px minmax(0,1fr) 300px}.rig-list{border-right:1px solid var(--border);background:var(--sidebar);padding:12px 8px;overflow:auto}.label{font-size:10px;color:var(--faint);letter-spacing:.13em;font-weight:700;padding:4px 7px 7px}.label.spacing{border-top:1px solid var(--border);margin-top:10px;padding-top:14px}.rig-list>button{width:100%;height:29px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:14px minmax(0,1fr) 24px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:12px;cursor:pointer}.rig-list>button.active,.rig-list>button:hover{background:var(--selected);color:var(--text)}.rig-list>button span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rig-list>button small{font-size:9px;color:var(--faint);text-align:right}.rig-list p{font-size:10px;line-height:1.45;color:var(--faint);padding:3px 7px}
  .field{display:block;font-size:10px;color:var(--faint);margin:8px 7px 0}.field input,.field select{display:block;width:100%;height:26px;box-sizing:border-box;margin-top:4px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}.field.checkbox-row{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:11px}.half-row input{width:60px}.checkbox-row input{height:auto;width:auto}.warnings{margin:12px 7px 0;border:1px solid #8f6c36;border-radius:5px;padding:7px}.warnings p{font-size:9px;line-height:1.4;color:#c8a45e;margin:0 0 3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .fit-list{display:flex;flex-direction:column;gap:3px;margin:2px 7px 0}.fit-row{height:26px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);display:grid;grid-template-columns:64px minmax(0,1fr) 30px;gap:6px;align-items:center;padding:0 7px;font:inherit;font-size:10px;cursor:pointer}.fit-row:hover{border-color:var(--border-strong);color:var(--text)}.fit-row.best{border-color:var(--accent);color:var(--text)}.fit-name{text-align:left;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.fit-bar{height:5px;border-radius:3px;background:var(--bg);overflow:hidden}.fit-bar i{display:block;height:100%;background:var(--accent);border-radius:3px}.fit-row small{font-size:9px;color:var(--faint);text-align:right}.fit-warning{font-size:9px;line-height:1.45;color:#c8a45e;margin:5px 7px 0}.apply-best{width:calc(100% - 14px);margin:8px 7px 0;height:25px;border:1px dashed var(--accent);border-radius:5px;background:transparent;color:var(--accent);font:inherit;font-size:10px;display:flex;align-items:center;justify-content:center;gap:5px;cursor:pointer}.apply-best:hover{background:var(--accent-dim)}.apply-best:disabled{opacity:.45;cursor:not-allowed}
  .hint{font-size:10px;line-height:1.5;color:var(--faint);padding:4px 2px}
</style>
