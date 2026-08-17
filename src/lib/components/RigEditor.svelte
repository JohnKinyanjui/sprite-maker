<script lang="ts">
  import { Bot, Bone, Check, Clapperboard, Copy, Crosshair, Layers, Pause, Play, Plus, Save, Sparkles, Trash2, Wand2, X } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage, RIG_MORPHOLOGIES, type Animation, type Asset, type ProviderStatus, type Rig, type RigBone, type RigFitReport, type RigFrame, type RigInput, type RigMorphology, type RigPoint, type RigPointKind, type RigSuggestion } from "$lib/types";

  let { workspaceId, worktreeId, assets, rigs, providers, selectedRigId, initialAssetId, onRigs, onSelected, onRendered, onPolish, onError, onNotice }: {
    workspaceId: string; worktreeId?: string; assets: Asset[]; rigs: Rig[]; providers: ProviderStatus[]; selectedRigId?: string;
    initialAssetId?: string; onRigs: (rigs: Rig[]) => void; onSelected: (id?: string) => void; onRendered: (animation: Animation, assetIds: string[]) => void;
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
  let dragging: { kind: "point" | "ghost"; id: string } | undefined;
  let idCounter = 0;
  const nextId = (prefix: string) => `${prefix}_${Date.now().toString(36)}_${++idCounter}`;
  let fit = $state<RigFitReport>();
  let fitBusy = $state(false);

  const masterAsset = $derived(assets.find(asset => asset.id === masterAssetId));
  const agentProviders = $derived(providers.filter(provider => provider.kind === "agent" && ["ready", "detected"].includes(provider.status)));
  const selectedPoint = $derived(points.find(point => point.id === selectedPointId));
  const selectedFrame = $derived(frames[frameIndex]);
  const showingPreview = $derived(previewPaths.length > 0 && previewIndex < previewPaths.length);
  const stageImage = $derived(showingPreview ? previewPaths[previewIndex] : masterAsset?.path);
  const pointKinds: RigPointKind[] = ["joint", "anchor", "contact", "pivot"];

  $effect(() => {
    if (initialAssetId && !selectedRigId) return;
    const target = rigs.find(rig => rig.id === (selectedRigId ?? rigs[0]?.id));
    if (target && target.id !== loadedRigId) loadRig(target);
  });
  $effect(() => {
    if (!initialAssetId || masterAssetId) return;
    masterAssetId = initialAssetId;
  });
  // The rig check is local pixel analysis, so it runs the moment a master is
  // chosen — it recommends (or rejects) a rig profile before any point work.
  $effect(() => {
    const id = masterAsset?.id;
    if (!id) { fit = undefined; return; }
    void runFit(id);
  });
  $effect(() => {
    if (!playing || !previewPaths.length) return;
    const timer = window.setTimeout(() => {
      previewIndex = previewIndex < previewPaths.length - 1 ? previewIndex + 1 : looping ? 0 : (playing = false, 0);
    }, 1000 / fps);
    return () => window.clearTimeout(timer);
  });

  function loadRig(rig: Rig) {
    loadedRigId = rig.id; rigId = rig.id; name = rig.name; morphology = rig.morphology; fps = rig.fps; looping = rig.looping;
    masterAssetId = rig.assetId; points = rig.points.map(point => ({ ...point })); bones = rig.bones.map(bone => ({ ...bone }));
    frames = rig.frames.map(frame => ({ ...frame, transforms: frame.transforms.map(t => ({ ...t })), contacts: frame.contacts.map(c => ({ ...c })) }));
    frameIndex = 0; selectedPointId = undefined; selectedBoneId = undefined; previewPaths = []; playing = false; suggestion = undefined; warnings = [];
  }
  function startNewDraft() {
    loadedRigId = undefined; rigId = undefined; name = "New rig"; morphology = "biped"; fps = 8; looping = true;
    points = []; bones = []; frames = []; frameIndex = 0; selectedPointId = undefined; selectedBoneId = undefined;
    previewPaths = []; playing = false; suggestion = undefined; warnings = [];
    onSelected(undefined);
  }
  function currentInput(): RigInput {
    return { id: rigId, workspaceId, worktreeId, assetId: masterAssetId, name, morphology, fps: Number(fps), looping, points, bones, frames };
  }
  async function refreshRigs(select?: string) { onRigs(await api.listRigs(workspaceId, worktreeId)); if (select) onSelected(select); }

  function addPointAt(x: number, y: number) {
    const index = points.length + 1;
    let candidate = `point_${index}`;
    let suffix = 1;
    while (points.some(point => point.name === candidate)) candidate = `point_${index}_${++suffix}`;
    const point: RigPoint = { id: nextId("p"), name: candidate, kind: "joint", x: Math.round(x * 10) / 10, y: Math.round(y * 10) / 10, confidence: 1, source: "user" };
    points = [...points, point]; selectedPointId = point.id; invalidatePreview();
  }
  function updatePoint(id: string, patch: Partial<RigPoint>) { points = points.map(point => point.id === id ? { ...point, ...patch } : point); invalidatePreview(); }
  function removePoint(id: string) {
    const target = points.find(point => point.id === id);
    points = points.filter(point => point.id !== id);
    if (target) bones = bones.filter(bone => bone.startPoint !== target.name && bone.endPoint !== target.name);
    if (selectedPointId === id) selectedPointId = undefined;
    invalidatePreview();
  }
  function addBone() {
    if (points.length < 2) { onError("Add at least two points before creating a bone"); return; }
    const index = bones.length + 1;
    let candidate = `bone_${index}`;
    while (bones.some(bone => bone.name === candidate)) candidate = `bone_${index}_${Math.random().toString(36).slice(2, 5)}`;
    const bone: RigBone = { id: nextId("b"), name: candidate, startPoint: points[0].name, endPoint: points[1].name, radius: 3, parent: undefined, z: 5 };
    bones = [...bones, bone]; selectedBoneId = bone.id; panelTab = "bones"; invalidatePreview();
  }
  function updateBone(id: string, patch: Partial<RigBone>) {
    bones = bones.map(bone => bone.id === id ? { ...bone, ...patch, parent: patch.parent === "" ? undefined : (patch.parent ?? bone.parent) } : bone);
    invalidatePreview();
  }
  function removeBone(id: string) {
    const target = bones.find(bone => bone.id === id);
    bones = bones.filter(bone => bone.id !== id);
    if (target) {
      frames = frames.map(frame => ({
        ...frame,
        transforms: frame.transforms.filter(t => t.bone !== target.name),
        contacts: frame.contacts.filter(c => c.bone !== target.name),
      }));
    }
    if (selectedBoneId === id) selectedBoneId = undefined;
    invalidatePreview();
  }
  function addFrame() { frames = [...frames, { phase: undefined, hold: false, rootDx: 0, rootDy: 0, transforms: [], contacts: [] }]; frameIndex = frames.length - 1; invalidatePreview(); }
  function duplicateFrame(index: number) { const copy = JSON.parse(JSON.stringify(frames[index])) as RigFrame; frames = [...frames.slice(0, index + 1), copy, ...frames.slice(index + 1)]; frameIndex = index + 1; invalidatePreview(); }
  function removeFrame(index: number) { frames = frames.filter((_, i) => i !== index); frameIndex = Math.max(0, Math.min(frameIndex, frames.length - 1)); invalidatePreview(); }
  function updateFrame(index: number, patch: Partial<RigFrame>) { frames = frames.map((frame, i) => i === index ? { ...frame, ...patch } : frame); invalidatePreview(); }
  function transformOf(frame: RigFrame, boneName: string) { return frame.transforms.find(t => t.bone === boneName); }
  function setTransform(frameIndexValue: number, boneName: string, patch: Partial<{ rotate: number; dx: number; dy: number }>) {
    const frame = frames[frameIndexValue];
    if (!frame) return;
    const existing = frame.transforms.find(t => t.bone === boneName);
    const next = existing ? { ...existing, ...patch } : { bone: boneName, dx: 0, dy: 0, rotate: 0, scaleX: 1, scaleY: 1, ...patch };
    frames = frames.map((item, i) => i === frameIndexValue ? { ...item, transforms: [...item.transforms.filter(t => t.bone !== boneName), next] } : item);
    invalidatePreview();
  }
  function addContact(frameIndexValue: number) {
    const frame = frames[frameIndexValue];
    if (!frame || !bones.length) return;
    updateFrame(frameIndexValue, { contacts: [...frame.contacts, { bone: bones[0].name, x: 0, y: 0, bend: 1 }] });
  }
  function updateContact(frameIndexValue: number, index: number, patch: Partial<{ bone: string; x: number; y: number; bend: number }>) {
    const frame = frames[frameIndexValue];
    if (!frame) return;
    updateFrame(frameIndexValue, { contacts: frame.contacts.map((contact, i) => i === index ? { ...contact, ...patch } : contact) });
  }
  function removeContact(frameIndexValue: number, index: number) {
    const frame = frames[frameIndexValue];
    if (!frame) return;
    updateFrame(frameIndexValue, { contacts: frame.contacts.filter((_, i) => i !== index) });
  }

  function invalidatePreview() { if (previewPaths.length) { previewPaths = []; playing = false; previewIndex = 0; } }

  async function runFit(assetId: string) {
    fitBusy = true;
    try {
      fit = await api.analyzeRigFit(assetId);
      if (fit.detections[0]) morphology = fit.detections[0].morphology;
    } catch { fit = undefined; } finally { fitBusy = false; }
  }
  function applyBestFit() {
    if (!fit) return;
    suggestion = fit.recommended;
    onNotice(`Best rig: ${fit.recommended.morphology} — drag the dashed points, then apply`);
  }
  function useMorphologyTemplate(value: RigMorphology) {
    morphology = value;
    void autoSuggest();
  }

  async function autoSuggest() {
    if (!masterAsset) { onError("Choose a source sprite first"); return; }
    suggesting = true;
    try {
      suggestion = await api.suggestRigPoints(masterAsset.id, morphology);
      onNotice(`Template placed ${suggestion.points.length} points — drag them onto the anatomy, then apply`);
    } catch (error) { onError(errorMessage(error)); } finally { suggesting = false; }
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
    morphology = suggestion.morphology;
    points = suggestion.points.map(point => ({ ...point }));
    bones = suggestion.bones.map(bone => ({ ...bone }));
    if (suggestion.frames.length) frames = suggestion.frames.map(frame => ({ ...frame, transforms: frame.transforms.map(t => ({ ...t })), contacts: frame.contacts.map(c => ({ ...c })) }));
    suggestion = undefined;
    invalidatePreview();
    onNotice("Suggestion applied to the canvas");
  }
  function dismissSuggestion() { suggestion = undefined; }

  async function preview() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before previewing"); return; }
    previewBusy = true;
    try {
      previewPaths = await api.renderRigPreview(currentInput());
      previewIndex = 0;
      onNotice(`Rendered ${previewPaths.length} preview frame${previewPaths.length === 1 ? "" : "s"}`);
    } catch (error) { onError(errorMessage(error)); } finally { previewBusy = false; }
  }
  async function save() {
    saving = true;
    try {
      const rig = await api.saveRig(currentInput());
      loadedRigId = rig.id; rigId = rig.id;
      await refreshRigs(rig.id);
      onNotice("Rig saved");
    } catch (error) { onError(errorMessage(error)); } finally { saving = false; }
  }
  async function removeRig() {
    if (!rigId) return;
    try {
      await api.deleteRig(rigId);
      await refreshRigs();
      startNewDraft();
      onNotice("Rig deleted");
    } catch (error) { onError(errorMessage(error)); }
  }
  async function renderAnimation() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before rendering"); return; }
    rendering = true;
    try {
      warnings = await api.validateRigSpec(currentInput());
      const result = await api.renderRigAnimation(currentInput());
      await refreshRigs();
      onRendered(result.animation, result.assetIds);
    } catch (error) { onError(errorMessage(error)); } finally { rendering = false; }
  }
  // Renders the rig deterministically, then hands the pose-canonical frames
  // to chat so the AI polishes detail without ever swapping leg identity.
  async function polishWithAi() {
    if (!masterAsset || !bones.length) { onError("Add at least one bone before polishing"); return; }
    rendering = true;
    try {
      warnings = await api.validateRigSpec(currentInput());
      const result = await api.renderRigAnimation(currentInput());
      await refreshRigs();
      onPolish(result.animation, result.assetIds);
    } catch (error) { onError(errorMessage(error)); } finally { rendering = false; }
  }

  function stageCoordinates(event: PointerEvent) {
    const stage = event.currentTarget as HTMLElement;
    const rect = stage.getBoundingClientRect();
    const master = masterAsset!;
    return { x: (event.clientX - rect.left) / scale, y: (event.clientY - rect.top) / scale };
  }
  function onStagePointerDown(event: PointerEvent) {
    if (event.target !== event.currentTarget) return;
    if (!masterAsset) return;
    const { x, y } = stageCoordinates(event);
    addPointAt(x, y);
  }
  function beginDrag(kind: "point" | "ghost", id: string, event: PointerEvent) {
    event.stopPropagation();
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    dragging = { kind, id };
    if (kind === "point") selectedPointId = id;
  }
  function onStagePointerMove(event: PointerEvent) {
    if (!dragging || !masterAsset) return;
    const { x, y } = stageCoordinates(event);
    const clampedX = Math.max(0, Math.min(masterAsset.width - 1, Math.round(x * 10) / 10));
    const clampedY = Math.max(0, Math.min(masterAsset.height - 1, Math.round(y * 10) / 10));
    if (dragging.kind === "point") updatePoint(dragging.id, { x: clampedX, y: clampedY });
    else if (suggestion) suggestion = { ...suggestion, points: suggestion.points.map(point => point.id === dragging!.id ? { ...point, x: clampedX, y: clampedY } : point) };
  }
  function endDrag() { dragging = undefined; }

  const ghostPoint = (point: RigPoint) => suggestion?.points.find(candidate => candidate.name === point.name) ?? point;
</script>

<section class="rig-editor">
  <header>
    <div><h1>Rig editor</h1><p>{rigs.length} rig{rigs.length === 1 ? "" : "s"} · {points.length} points · {bones.length} bones · {frames.length} pose frame{frames.length === 1 ? "" : "s"}</p></div>
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
      {#each rigs as rig}<button class:active={rig.id === rigId} onclick={() => onSelected(rig.id)}><Bone size={13}/><span>{rig.name}</span><small>{rig.bones.length}</small></button>{/each}
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
    <div class="workspace">
      <div class="canvas-wrap">
        {#if masterAsset}
          <div class="stage" class:ghosting={Boolean(suggestion)} style={`width:${masterAsset.width * scale}px;height:${masterAsset.height * scale}px`}
            onpointerdown={onStagePointerDown} onpointermove={onStagePointerMove} onpointerup={endDrag} onpointercancel={endDrag} role="presentation">
            {#if stageImage}<img src={assetUrl(stageImage)} alt={masterAsset.name} draggable="false"/>{/if}
            <svg width="100%" height="100%" viewBox={`0 0 ${masterAsset.width} ${masterAsset.height}`} preserveAspectRatio="none">
              {#each bones as bone (bone.id)}
                {@const start = ghostPoint(points.find(point => point.name === bone.startPoint) ?? { x: -10, y: -10, name: bone.startPoint, id: bone.id, kind: "joint", confidence: 0, source: "user" } as RigPoint)}
                {@const end = ghostPoint(points.find(point => point.name === bone.endPoint) ?? { x: -10, y: -10, name: bone.endPoint, id: bone.id, kind: "joint", confidence: 0, source: "user" } as RigPoint)}
                <line x1={start.x} y1={start.y} x2={end.x} y2={end.y} class="bone" class:near={bone.z >= 7} class:far={bone.z <= 3} vector-effect="non-scaling-stroke"/>
                <circle cx={start.x} cy={start.y} r={bone.radius} class="capsule"/>
              {/each}
              {#if suggestion}{#each suggestion.points as point (point.id)}
                <circle cx={point.x} cy={point.y} r={Math.max(1.6, 4 / scale)} class="point ghost" class:selected={point.id === selectedPointId} role="button" tabindex="-1"
                  onpointerdown={(event) => beginDrag("ghost", point.id, event)}/>
                {#if scale >= 2}<text x={point.x + 5 / scale} y={point.y - 4 / scale} class="point-label ghost">{point.name}</text>{/if}
              {/each}{/if}
              {#each points as point (point.id)}
                <circle cx={point.x} cy={point.y} r={Math.max(1.8, 4.5 / scale)} class="point" class:selected={point.id === selectedPointId} class:contact={point.kind === "contact"} role="button" tabindex="-1"
                  onpointerdown={(event) => beginDrag("point", point.id, event)}/>
                {#if scale >= 2}<text x={point.x + 6 / scale} y={point.y - 5 / scale} class="point-label">{point.name}</text>{/if}
              {/each}
            </svg>
          </div>
        {:else}
          <div class="empty"><Crosshair size={26}/><span>Choose a source sprite, then place points</span><small>Click the canvas to add a point, or start from a template.</small></div>
        {/if}
      </div>
      <div class="toolbar">
        <button onclick={autoSuggest} disabled={!masterAsset || suggesting || aiBusy}><Wand2 size={12}/>{suggesting ? "Placing…" : "Auto place"}</button>
        <div class="ai-group">
          <button onclick={aiSuggest} disabled={!masterAsset || aiBusy || suggesting || !agentProviders.length} class="accent"><Bot size={12}/>{aiBusy ? "Asking…" : "Ask AI"}</button>
          <select bind:value={aiProviderId} aria-label="AI provider" disabled={aiBusy}>
            {#each agentProviders as provider}<option value={provider.id}>{provider.name}</option>{/each}
          </select>
          <input class="motion" placeholder="Motion intent, e.g. walk cycle (optional — also proposes poses)" bind:value={aiMotion} disabled={aiBusy}/>
        </div>
        <label class="zoom">Zoom<select bind:value={scale}>{#each [1, 2, 3, 4, 6, 8] as value}<option value={value}>{value}×</option>{/each}</select></label>
        <div class="spacer"></div>
        <button onclick={preview} disabled={!masterAsset || !bones.length || previewBusy || rendering}>{previewBusy ? "Rendering…" : "Preview"}</button>
        <button class="play" onclick={() => { if (!previewPaths.length) void preview(); playing = !playing; }} disabled={!previewPaths.length} title={playing ? "Pause preview" : "Play preview"}>{#if playing}<Pause size={13}/>{:else}<Play size={13} fill="currentColor"/>{/if}</button>
        {#if previewPaths.length}<span class="preview-count">{previewIndex + 1} / {previewPaths.length}</span>{/if}
      </div>
      {#if suggestion}
        <div class="suggestion">
          <div class="suggestion-text"><strong>{suggestion.source === "ai" ? "AI suggestion" : "Template suggestion"}</strong><span>{suggestion.points.length} points · {suggestion.bones.length} bones{#if suggestion.frames.length} · {suggestion.frames.length} pose frames{/if} — drag the dashed points to fine-tune.</span>{#if suggestion.reasoning}<p>{suggestion.reasoning}</p>{/if}</div>
          <button class="primary" onclick={applySuggestion}><Check size={12}/> Apply</button>
          <button onclick={dismissSuggestion}><X size={12}/> Dismiss</button>
        </div>
      {/if}
    </div>
    <aside class="panel">
      <nav>
        <button class:active={panelTab === "points"} onclick={() => panelTab = "points"}>Points</button>
        <button class:active={panelTab === "bones"} onclick={() => panelTab = "bones"}>Bones</button>
        <button class:active={panelTab === "frames"} onclick={() => panelTab = "frames"}>Poses</button>
      </nav>
      <div class="panel-body">
        {#if panelTab === "points"}
          {#if selectedPoint}
            <div class="inspector">
              <label>Name<input value={selectedPoint.name} onchange={(event) => updatePoint(selectedPoint.id, { name: event.currentTarget.value })}/></label>
              <label>Kind<select value={selectedPoint.kind} onchange={(event) => updatePoint(selectedPoint.id, { kind: event.currentTarget.value as RigPointKind })}>{#each pointKinds as kind}<option value={kind}>{kind}</option>{/each}</select></label>
              <div class="pair"><label>X<input type="number" step="0.5" value={selectedPoint.x} onchange={(event) => updatePoint(selectedPoint.id, { x: Number(event.currentTarget.value) })}/></label>
              <label>Y<input type="number" step="0.5" value={selectedPoint.y} onchange={(event) => updatePoint(selectedPoint.id, { y: Number(event.currentTarget.value) })}/></label></div>
              <p class="meta">{selectedPoint.source} suggestion · {Math.round(selectedPoint.confidence * 100)}% confidence{selectedPoint.note ? ` · ${selectedPoint.note}` : ""}</p>
              <button class="danger" onclick={() => removePoint(selectedPoint!.id)}><Trash2 size={11}/> Remove point</button>
            </div>
          {/if}
          <div class="item-list">
            {#each points as point (point.id)}
              <button class:active={point.id === selectedPointId} onclick={() => { selectedPointId = point.id; }}>
                <i class="kind" class:contact={point.kind === "contact"}></i><span>{point.name}</span><small>{point.kind}</small><small class="coords">{Math.round(point.x)},{Math.round(point.y)}</small>
              </button>
            {/each}
            {#if !points.length}<p class="hint">Click the canvas or use Auto place / Ask AI.</p>{/if}
          </div>
        {:else if panelTab === "bones"}
          <button class="add-row" onclick={addBone}><Plus size={12}/> Add bone</button>
          <div class="item-list tall">
            {#each bones as bone (bone.id)}
              <div class="bone-card" class:active={bone.id === selectedBoneId}>
                <header role="button" tabindex="0" onclick={() => selectedBoneId = bone.id} onkeydown={(event) => { if (event.key === "Enter") selectedBoneId = bone.id; }}><Layers size={12}/><input value={bone.name} onchange={(event) => updateBone(bone.id, { name: event.currentTarget.value })}/><small>z {bone.z}</small></header>
                <div class="bone-grid">
                  <label>Start<select value={bone.startPoint} onchange={(event) => updateBone(bone.id, { startPoint: event.currentTarget.value })}>{#each points as point}<option value={point.name}>{point.name}</option>{/each}</select></label>
                  <label>End<select value={bone.endPoint} onchange={(event) => updateBone(bone.id, { endPoint: event.currentTarget.value })}>{#each points as point}<option value={point.name}>{point.name}</option>{/each}</select></label>
                  <label>Radius<input type="number" min="0.5" max="64" step="0.5" value={bone.radius} onchange={(event) => updateBone(bone.id, { radius: Number(event.currentTarget.value) })}/></label>
                  <label>Layer<input type="number" step="1" value={bone.z} onchange={(event) => updateBone(bone.id, { z: Number(event.currentTarget.value) })}/></label>
                  <label class="wide">Parent<select value={bone.parent ?? ""} onchange={(event) => updateBone(bone.id, { parent: event.currentTarget.value })}><option value="">none</option>{#each bones.filter(candidate => candidate.id !== bone.id) as candidate}<option value={candidate.name}>{candidate.name}</option>{/each}</select></label>
                  <button class="danger icon" onclick={() => removeBone(bone.id)} title="Remove bone"><Trash2 size={11}/></button>
                </div>
              </div>
            {/each}
            {#if !bones.length}<p class="hint">Bones are capsules between two points. The renderer claims every pixel inside a capsule, and leftovers go to the nearest bone.</p>{/if}
          </div>
        {:else}
          <div class="frame-strip">
            {#each frames as frame, index (index)}
              <button class:active={index === frameIndex} onclick={() => frameIndex = index}>
                <strong>{String(index + 1).padStart(2, "0")}</strong>
                <span>{frame.phase || (frame.hold ? "hold" : "pose")}</span>
              </button>
            {/each}
            <button class="add" onclick={addFrame} title="Add frame"><Plus size={12}/></button>
          </div>
          {#if selectedFrame}
            <div class="inspector">
              <div class="pair">
                <label>Phase<input value={selectedFrame.phase ?? ""} placeholder="contact" onchange={(event) => updateFrame(frameIndex, { phase: event.currentTarget.value || undefined })}/></label>
                <label class="checkbox-row"><input type="checkbox" checked={selectedFrame.hold} onchange={(event) => updateFrame(frameIndex, { hold: event.currentTarget.checked })}/> Hold</label>
              </div>
              <div class="pair">
                <label>Root dx<input type="number" step="1" value={selectedFrame.rootDx} onchange={(event) => updateFrame(frameIndex, { rootDx: Number(event.currentTarget.value) })}/></label>
                <label>Root dy<input type="number" step="1" value={selectedFrame.rootDy} onchange={(event) => updateFrame(frameIndex, { rootDy: Number(event.currentTarget.value) })}/></label>
              </div>
            </div>
            <div class="transform-list">
              <header><span>BONE</span><span>ROTATE°</span><span>DX</span><span>DY</span></header>
              {#each bones as bone (bone.id)}
                {@const transform = transformOf(selectedFrame, bone.name)}
                <div class="transform-row">
                  <span title={bone.name}>{bone.name}</span>
                  <input type="number" step="1" value={transform?.rotate ?? 0} onchange={(event) => setTransform(frameIndex, bone.name, { rotate: Number(event.currentTarget.value) })}/>
                  <input type="number" step="1" value={transform?.dx ?? 0} onchange={(event) => setTransform(frameIndex, bone.name, { dx: Number(event.currentTarget.value) })}/>
                  <input type="number" step="1" value={transform?.dy ?? 0} onchange={(event) => setTransform(frameIndex, bone.name, { dy: Number(event.currentTarget.value) })}/>
                </div>
              {/each}
            </div>
            <div class="contacts">
              <header><span>PLANTED CONTACTS</span><button onclick={() => addContact(frameIndex)} disabled={!bones.length}><Plus size={11}/> Pin</button></header>
              {#each selectedFrame.contacts as contact, index}
                <div class="contact-row">
                  <select value={contact.bone} onchange={(event) => updateContact(frameIndex, index, { bone: event.currentTarget.value })}>{#each bones as bone}<option value={bone.name}>{bone.name}</option>{/each}</select>
                  <input type="number" step="0.5" value={contact.x} title="Target x" onchange={(event) => updateContact(frameIndex, index, { x: Number(event.currentTarget.value) })}/>
                  <input type="number" step="0.5" value={contact.y} title="Target y" onchange={(event) => updateContact(frameIndex, index, { y: Number(event.currentTarget.value) })}/>
                  <select value={contact.bend >= 0 ? "1" : "-1"} title="Bend direction" onchange={(event) => updateContact(frameIndex, index, { bend: Number(event.currentTarget.value) })}><option value="1">↷</option><option value="-1">↶</option></select>
                  <button class="danger icon" onclick={() => removeContact(frameIndex, index)} title="Remove contact"><Trash2 size={11}/></button>
                </div>
              {/each}
              {#if !selectedFrame.contacts.length}<p class="hint">Contacts plant a bone's end point in place with two-bone IK — feet stop sliding.</p>{/if}
            </div>
          {:else}
            <p class="hint">Add pose frames to keyframe rotations per bone. Preview or render at any time.</p>
          {/if}
        {/if}
      </div>
    </aside>
  </div>
</section>

<style>
  .rig-editor{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 13px 0 17px}header h1{font-size:12px;margin:0}header p{font-size:11px;color:var(--faint);margin:3px 0 0}.actions{display:flex;gap:5px}.actions button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:11px;cursor:pointer}.actions button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}.actions button.danger{color:#cc7a74}button:disabled{opacity:.4;cursor:not-allowed}
  .body{flex:1;min-height:0;display:grid;grid-template-columns:212px minmax(0,1fr) 300px}.rig-list{border-right:1px solid var(--border);background:var(--sidebar);padding:12px 8px;overflow:auto}.label{font-size:10px;color:var(--faint);letter-spacing:.13em;font-weight:700;padding:4px 7px 7px}.label.spacing{border-top:1px solid var(--border);margin-top:10px;padding-top:14px}.rig-list>button{width:100%;height:29px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:14px minmax(0,1fr) 24px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:12px;cursor:pointer}.rig-list>button.active,.rig-list>button:hover{background:var(--selected);color:var(--text)}.rig-list>button span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.rig-list>button small{font-size:9px;color:var(--faint);text-align:right}.rig-list p{font-size:10px;line-height:1.45;color:var(--faint);padding:3px 7px}
  .field{display:block;font-size:10px;color:var(--faint);margin:8px 7px 0}.field input,.field select{display:block;width:100%;height:26px;box-sizing:border-box;margin-top:4px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}.field.checkbox-row,.pair .checkbox-row{display:flex;align-items:center;gap:6px;color:var(--muted);font-size:11px}.half-row input{width:60px}.checkbox-row input{height:auto;width:auto}.warnings{margin:12px 7px 0;border:1px solid #8f6c36;border-radius:5px;padding:7px}.warnings p{font-size:9px;line-height:1.4;color:#c8a45e;margin:0 0 3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .fit-list{display:flex;flex-direction:column;gap:3px;margin:2px 7px 0}.fit-row{height:26px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);display:grid;grid-template-columns:64px minmax(0,1fr) 30px;gap:6px;align-items:center;padding:0 7px;font:inherit;font-size:10px;cursor:pointer}.fit-row:hover{border-color:var(--border-strong);color:var(--text)}.fit-row.best{border-color:var(--accent);color:var(--text)}.fit-name{text-align:left;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.fit-bar{height:5px;border-radius:3px;background:var(--bg);overflow:hidden}.fit-bar i{display:block;height:100%;background:var(--accent);border-radius:3px}.fit-row small{font-size:9px;color:var(--faint);text-align:right}.fit-warning{font-size:9px;line-height:1.45;color:#c8a45e;margin:5px 7px 0}.apply-best{width:calc(100% - 14px);margin:8px 7px 0;height:25px;border:1px dashed var(--accent);border-radius:5px;background:transparent;color:var(--accent);font:inherit;font-size:10px;display:flex;align-items:center;justify-content:center;gap:5px;cursor:pointer}.apply-best:hover{background:var(--accent-dim)}.apply-best:disabled{opacity:.45;cursor:not-allowed}
  .workspace{min-width:0;min-height:0;display:flex;flex-direction:column}.canvas-wrap{flex:1;min-height:0;overflow:auto;display:grid;place-items:center;padding:18px;background:var(--bg)}.stage{position:relative;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border:1px solid var(--border-strong);box-shadow:0 14px 36px #0005;touch-action:none;cursor:crosshair}.stage img{position:absolute;inset:0;width:100%;height:100%;object-fit:fill;image-rendering:pixelated;pointer-events:none}.stage svg{position:absolute;inset:0;width:100%;height:100%;pointer-events:none;overflow:visible}.stage .bone{stroke:#69d2c5;stroke-width:2;stroke-dasharray:none;opacity:.85}.stage .bone.near{stroke:#b8cf69}.stage .bone.far{stroke:#8a95a5}.stage .capsule{fill:#69d2c522;stroke:#69d2c566;stroke-width:1}.stage .point{fill:#69d2c5;stroke:#10110f;stroke-width:1.5;cursor:grab;pointer-events:all}.stage .point.contact{fill:#e0a458}.stage .point.selected{stroke:#fff;stroke-width:2.5}.stage .point.ghost{fill:#ffffff00;stroke:#69d2c5;stroke-width:1.5;stroke-dasharray:3 2;cursor:grab;pointer-events:all}.stage .point.ghost.selected{stroke-width:2.5}.stage .point-label{fill:#e8e7e2;stroke:#10110fcc;stroke-width:2.5px;paint-order:stroke;font-size:2.4px;font-family:inherit;pointer-events:none}.stage .point-label.ghost{fill:#9fe8de}.stage.ghosting{outline:1px dashed #69d2c5;outline-offset:3px}.empty{display:flex;flex-direction:column;gap:8px;align-items:center;color:var(--faint);font-size:11px}.empty small{font-size:10px;max-width:260px;text-align:center;line-height:1.5}
  .toolbar{border-top:1px solid var(--border);background:var(--sidebar);min-height:40px;display:flex;align-items:center;gap:6px;padding:5px 12px;flex-wrap:wrap}.toolbar button{height:27px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:5px;padding:0 9px;font:inherit;font-size:11px;cursor:pointer}.toolbar button.accent{border-color:var(--accent);color:var(--accent)}.toolbar button.play{width:32px;justify-content:center;background:var(--text);color:var(--bg)}.toolbar .spacer{flex:1}.toolbar select,.toolbar .motion{height:27px;box-sizing:border-box;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}.toolbar .motion{width:250px}.ai-group{display:flex;gap:4px;align-items:center}.zoom{display:flex;align-items:center;gap:5px;font-size:10px;color:var(--faint)}.zoom select{height:25px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 4px}.preview-count{font-size:10px;color:var(--faint);margin-left:4px}
  .suggestion{border-top:1px solid var(--accent);background:var(--accent-dim);display:flex;align-items:center;gap:10px;padding:9px 14px}.suggestion-text{flex:1;min-width:0}.suggestion-text strong{font-size:11px}.suggestion-text span{font-size:10px;color:var(--muted);margin-left:8px}.suggestion-text p{font-size:10px;line-height:1.45;color:var(--muted);margin:4px 0 0;max-height:32px;overflow:hidden}.suggestion button{height:27px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);font:inherit;font-size:11px;display:flex;align-items:center;gap:5px;padding:0 9px;cursor:pointer}.suggestion button.primary{background:var(--accent);border-color:var(--accent);color:#10110f}
  .panel{border-left:1px solid var(--border);background:var(--sidebar);display:flex;flex-direction:column;min-height:0}.panel nav{display:flex;border-bottom:1px solid var(--border);height:33px;flex:0 0 auto}.panel nav button{flex:1;border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;letter-spacing:.08em;cursor:pointer;border-bottom:2px solid transparent}.panel nav button.active{color:var(--text);border-bottom-color:var(--accent)}.panel-body{flex:1;min-height:0;overflow:auto;padding:10px}
  .inspector{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:9px;margin-bottom:10px}.inspector label{display:block;font-size:9px;color:var(--faint);letter-spacing:.06em;margin-top:6px}.inspector label:first-child{margin-top:0}.inspector input,.inspector select{display:block;width:100%;height:24px;box-sizing:border-box;margin-top:3px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 5px;outline:0}.inspector .pair{display:grid;grid-template-columns:1fr 1fr;gap:6px}.inspector .meta{font-size:9px;color:var(--faint);margin:7px 0 0}.inspector button.danger{margin-top:8px;height:24px;border:1px solid var(--border);border-radius:4px;background:transparent;color:#cc7a74;font:inherit;font-size:10px;display:flex;align-items:center;gap:5px;padding:0 7px;cursor:pointer}
  .item-list{display:flex;flex-direction:column;gap:3px}.item-list.tall{gap:6px}.item-list>button{height:27px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:10px minmax(0,1fr) 44px 52px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:11px;cursor:pointer}.item-list>button.active,.item-list>button:hover{background:var(--selected);color:var(--text)}.item-list>button i.kind{width:7px;height:7px;border-radius:50%;background:#69d2c5}.item-list>button i.kind.contact{background:#e0a458}.item-list small{font-size:9px;color:var(--faint)}.coords{text-align:right}.hint{font-size:10px;line-height:1.5;color:var(--faint);padding:4px 2px}
  .add-row{height:26px;border:1px dashed var(--border-strong);border-radius:5px;background:transparent;color:var(--muted);font:inherit;font-size:10px;display:flex;align-items:center;justify-content:center;gap:5px;width:100%;cursor:pointer;margin-bottom:8px}
  .bone-card{border:1px solid var(--border);border-radius:6px;background:var(--surface);padding:7px}.bone-card.active{border-color:var(--accent)}.bone-card header{display:flex;align-items:center;gap:6px;height:24px}.bone-card header :global(svg){color:var(--faint);flex:0 0 auto}.bone-card header input{flex:1;min-width:0;height:23px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 5px;outline:0}.bone-card header small{font-size:9px;color:var(--faint)}.bone-grid{display:grid;grid-template-columns:1fr 1fr 1fr 26px;gap:5px;margin-top:7px;align-items:end}.bone-grid label{font-size:9px;color:var(--faint);display:block}.bone-grid input,.bone-grid select{display:block;width:100%;height:22px;box-sizing:border-box;margin-top:3px;background:var(--bg);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:0 4px;outline:0}.bone-grid .wide{grid-column:span 3}.bone-grid button.icon,.contact-row button.icon{height:22px;width:26px;border:1px solid var(--border);border-radius:4px;background:transparent;color:#cc7a74;display:grid;place-items:center;cursor:pointer}
  .frame-strip{display:flex;gap:4px;flex-wrap:wrap;margin-bottom:10px}.frame-strip>button{min-width:46px;height:38px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:1px;font:inherit;cursor:pointer}.frame-strip>button strong{font-size:11px}.frame-strip>button span{font-size:8px;max-width:56px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.frame-strip>button.active{border-color:var(--accent);color:var(--text)}.frame-strip>button.add{justify-content:center}
  .transform-list{border:1px solid var(--border);border-radius:6px;overflow:hidden;margin-top:10px}.transform-list header,.transform-row{display:grid;grid-template-columns:minmax(0,1fr) 48px 40px 40px;gap:4px;padding:4px 6px;align-items:center}.transform-list header{background:var(--surface);font-size:8px;letter-spacing:.08em;color:var(--faint)}.transform-row{border-top:1px solid var(--border);font-size:10px;color:var(--muted)}.transform-row span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.transform-row input{width:100%;height:21px;box-sizing:border-box;background:var(--bg);border:1px solid var(--border);border-radius:3px;color:var(--text);font:inherit;font-size:10px;padding:0 3px;outline:0}
  .contacts{margin-top:12px}.contacts header{display:flex;align-items:center;justify-content:space-between;font-size:9px;letter-spacing:.1em;color:var(--faint);margin-bottom:6px}.contacts header button{height:22px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--muted);font:inherit;font-size:9px;display:flex;align-items:center;gap:4px;padding:0 6px;cursor:pointer}.contact-row{display:grid;grid-template-columns:minmax(0,1fr) 42px 42px 32px 26px;gap:4px;margin-bottom:4px}.contact-row select,.contact-row input{height:22px;box-sizing:border-box;background:var(--bg);border:1px solid var(--border);border-radius:3px;color:var(--text);font:inherit;font-size:10px;padding:0 3px;outline:0}
</style>
