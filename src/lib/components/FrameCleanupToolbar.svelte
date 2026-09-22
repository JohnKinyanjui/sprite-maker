<script lang="ts">
  import { Eraser, RotateCcw } from "lucide-svelte";
  import { api } from "$lib/api";
  import { errorMessage, type AssetVersion, type BrushStamp } from "$lib/types";

  let {
    assetId,
    strokes,
    busy = false,
    versions = [],
    onApplied,
    onError,
    onNotice,
  }: {
    assetId: string;
    strokes: BrushStamp[];
    busy?: boolean;
    versions?: AssetVersion[];
    onApplied: () => void | Promise<void>;
    onError: (message: string) => void;
    onNotice?: (message: string) => void;
  } = $props();

  let working = $state(false);

  async function erase() {
    if (!strokes.length) {
      onError("Disegna almeno un tratto con il brush sulla preview");
      return;
    }
    working = true;
    try {
      const result = await api.paintFrameAlpha({ assetId, strokes, mode: "erase" });
      onNotice?.(`Gomma applicata — opachi rimasti: ${result.opaquePixelCount}`);
      await onApplied();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      working = false;
    }
  }

  async function restoreLast() {
    const previous = versions.find((version) => version.selected === false) ?? versions[1];
    if (!previous) {
      onError("Nessuna versione precedente da ripristinare");
      return;
    }
    working = true;
    try {
      await api.restoreAssetVersion(assetId, previous.id);
      onNotice?.("Versione precedente ripristinata");
      await onApplied();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      working = false;
    }
  }
</script>

<div class="cleanup-toolbar">
  <button disabled={busy || working || !strokes.length} onclick={erase}><Eraser size={12} /> Applica gomma</button>
  <button disabled={busy || working || versions.length < 2} onclick={restoreLast}><RotateCcw size={12} /> Ripristina versione</button>
</div>

<style>
  .cleanup-toolbar{display:flex;gap:8px;flex-wrap:wrap}
  button{height:28px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--muted);font:inherit;font-size:10px;padding:0 10px;display:flex;align-items:center;gap:5px;cursor:pointer}
  button:disabled{opacity:.45}
</style>
