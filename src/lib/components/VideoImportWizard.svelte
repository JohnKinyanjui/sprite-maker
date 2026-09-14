<script lang="ts">
  import { ChevronLeft, ChevronRight, Film, X } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import { errorMessage } from "$lib/types";

  let {
    workspaceId,
    videoPath,
    busy = false,
    onComplete,
    onClose,
    onError,
  }: {
    workspaceId: string;
    videoPath: string;
    busy?: boolean;
    onComplete: (assetIds: string[], options: { normalized: boolean; hardened: boolean; fps: number }) => void | Promise<void>;
    onClose: () => void;
    onError: (message: string) => void;
  } = $props();

  let step = $state(1);
  let extractFps = $state(10);
  let runNormalize = $state(true);
  let runHarden = $state(false);
  let working = $state(false);
  let extractedAssetIds = $state<string[]>([]);
  let selectedIds = $state<Set<string>>(new Set());
  let framePreviews = $state<{ assetId: string; path: string }[]>([]);
  const fileName = $derived(videoPath.split(/[\\/]/).pop() ?? videoPath);

  async function extractFrames() {
    working = true;
    try {
      const result = await api.extractVideoFrames({ workspaceId, videoPath, fps: extractFps });
      extractedAssetIds = result.assetIds;
      const subsample = await api.subsampleVideoFrames({ workspaceId, assetIds: result.assetIds });
      selectedIds = new Set(subsample.keptAssetIds);
      framePreviews = result.assetIds.map((assetId, index) => ({
        assetId,
        path: result.framePaths[index] ?? "",
      }));
      step = 4;
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      working = false;
    }
  }

  function toggleFrame(assetId: string) {
    const next = new Set(selectedIds);
    if (next.has(assetId)) next.delete(assetId);
    else next.add(assetId);
    selectedIds = next;
  }

  async function finishImport() {
    const assetIds = extractedAssetIds.filter(id => selectedIds.has(id));
    if (!assetIds.length) {
      onError("Seleziona almeno un frame da importare");
      return;
    }
    working = true;
    try {
      await onComplete(assetIds, { normalized: runNormalize, hardened: runHarden, fps: extractFps });
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      working = false;
    }
  }

  const next = async () => {
    if (step === 1) step = 2;
    else if (step === 2) step = 3;
    else if (step === 3) await extractFrames();
    else await finishImport();
  };

  const back = () => {
    if (step > 1) step -= 1;
  };
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <section class="dialog" class:wide={step === 4}>
    <header>
      <div>
        <p>VIDEO IMPORT</p>
        <h2>Sprite wizard</h2>
        <small>Step {step} of 4 — {fileName}</small>
      </div>
      <button onclick={onClose}><X size={15} /></button>
    </header>

    {#if step === 1}
      <div class="panel"><Film size={28} /><strong>Source video selected</strong><p>ffmpeg extracts PNG frames at a fixed FPS into <code>assets/imports/</code>. Step 4 lets you curate which frames become the animation.</p></div>
    {:else if step === 2}
      <div class="form"><label>Extract FPS<input type="number" min="1" max="60" bind:value={extractFps} /></label><p class="hint">Typical sprite loops use 8–12 FPS. Higher values create more frames.</p></div>
    {:else if step === 3}
      <div class="form">
        <p class="hint">About {Math.max(1, Math.round(extractFps))} frames per second will be extracted from the video.</p>
        <label class="check"><input type="checkbox" bind:checked={runNormalize} /> Normalize to anchor after import</label>
        <label class="check"><input type="checkbox" bind:checked={runHarden} /> Run production harden after import</label>
        <p class="hint">Normalize requires a promoted anchor on the active animation. Harden runs clean alpha, normalize, and contract check.</p>
      </div>
    {:else}
      <div class="filmstrip">
        <p class="hint">Auto-subsample selected the likely keyframes. Toggle keep/drop before finishing.</p>
        <div class="frames">
          {#each framePreviews as frame, index}
            <button class:selected={selectedIds.has(frame.assetId)} onclick={() => toggleFrame(frame.assetId)}>
              <span>{index + 1}</span>
              {#if frame.path}<img src={assetUrl(frame.path)} alt={`Frame ${index + 1}`} />{/if}
            </button>
          {/each}
        </div>
        <p class="hint">{selectedIds.size} / {framePreviews.length} frames selected</p>
      </div>
    {/if}

    <footer>
      {#if step > 1 && step < 4}<button onclick={back} disabled={working || busy}><ChevronLeft size={12} /> Back</button>{/if}
      <button onclick={onClose} disabled={working || busy}>Cancel</button>
      <button class="primary" disabled={working || busy} onclick={next}>
        {#if step < 3}<ChevronRight size={12} /> Next{:else if step === 3}{working || busy ? "Extracting…" : "Extract frames"}{:else}{working || busy ? "Importing…" : "Finish import"}{/if}
      </button>
    </footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:70;background:#000a;display:grid;place-items:center}
  .dialog{width:min(520px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;box-shadow:0 28px 90px #000b;padding:20px}
  .dialog.wide{width:min(760px,calc(100vw - 30px))}
  header{display:flex;align-items:flex-start;justify-content:space-between}
  header p{font-size:9px;letter-spacing:.15em;font-weight:700;color:var(--accent);margin:0 0 7px}
  h2{font-size:17px;margin:0}
  header small{display:block;font-size:10px;color:var(--faint);margin-top:6px}
  header button{border:0;background:transparent;color:var(--faint);cursor:pointer}
  .panel{display:grid;justify-items:center;text-align:center;gap:8px;padding:24px 12px;color:var(--muted)}
  .panel strong{font-size:12px;color:var(--text)}
  .panel p{font-size:10px;line-height:1.45;max-width:360px}
  .panel code{font-size:9px}
  .form,.filmstrip{margin-top:16px;display:grid;gap:12px}
  label{font-size:10px;color:var(--muted)}
  input{width:100%;margin-top:5px;background:var(--bg);border:1px solid var(--border);border-radius:5px;color:var(--text);font:inherit;font-size:11px;padding:8px;outline:0;height:32px;padding:0 8px;box-sizing:border-box}
  .check{display:flex;align-items:center;gap:8px}
  .check input{width:auto;margin:0;height:auto}
  .hint{font-size:9px;color:var(--faint);line-height:1.4;margin:0}
  .frames{display:flex;gap:8px;overflow-x:auto;padding-bottom:6px}
  .frames button{width:72px;min-width:72px;border:1px solid var(--border);border-radius:6px;background:var(--bg);padding:4px;cursor:pointer;display:grid;gap:4px}
  .frames button.selected{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}
  .frames span{font-size:9px;color:var(--faint);text-align:center}
  .frames img{width:100%;height:56px;object-fit:contain;image-rendering:pixelated;background:var(--preview)}
  footer{display:flex;justify-content:flex-end;gap:7px;border-top:1px solid var(--border);padding-top:15px;margin-top:18px}
  footer button{height:31px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);font:inherit;font-size:11px;padding:0 11px;cursor:pointer;display:flex;align-items:center;gap:4px}
  footer button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}
  button:disabled{opacity:.45}
</style>
