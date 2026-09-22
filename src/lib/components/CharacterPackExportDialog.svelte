<script lang="ts">
  import { Package, X } from "lucide-svelte";

  let {
    busy = false,
    onClose,
    onExport,
  }: {
    busy?: boolean;
    onClose: () => void;
    onExport: (options: { includeAnimatedPreviews: boolean }) => void | Promise<void>;
  } = $props();

  let includeGif = $state(false);
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <section class="dialog">
    <header>
      <div><p>EXPORT</p><h2>Character pack</h2></div>
      <button onclick={onClose}><X size={15} /></button>
    </header>
    <label class="check"><input type="checkbox" bind:checked={includeGif} /> Includi preview animate (GIF)</label>
    <p class="hint">Le GIF vengono scritte in <code>previews/</code> accanto alle still PNG.</p>
    <footer>
      <button onclick={onClose} disabled={busy}>Annulla</button>
      <button class="primary" disabled={busy} onclick={() => onExport({ includeAnimatedPreviews: includeGif })}><Package size={12} />{busy ? "Export…" : "Export pack"}</button>
    </footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;background:#0009;display:grid;place-items:center;z-index:50}
  .dialog{width:min(380px,calc(100vw - 24px));background:var(--surface);border:1px solid var(--border-strong);border-radius:8px;padding:16px}
  header{display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:12px}
  header p{font-size:9px;letter-spacing:.14em;color:var(--faint);margin:0 0 4px}
  header h2{font-size:15px;margin:0}
  header button{border:0;background:transparent;color:var(--faint);cursor:pointer}
  .check{font-size:11px;color:var(--muted);display:flex;align-items:center;gap:8px}
  .hint{font-size:10px;color:var(--faint);line-height:1.45}
  footer{display:flex;justify-content:flex-end;gap:8px;margin-top:16px}
  footer button{height:30px;border:1px solid var(--border);background:var(--bg);color:var(--muted);border-radius:4px;font:inherit;font-size:11px;padding:0 10px;cursor:pointer;display:flex;align-items:center;gap:6px}
  footer .primary{background:var(--text);color:var(--bg);border-color:var(--text)}
</style>
