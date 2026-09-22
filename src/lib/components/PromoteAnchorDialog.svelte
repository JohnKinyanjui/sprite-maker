<script lang="ts">
  import { X } from "lucide-svelte";

  const presets = [
    { id: "side", label: "Side platformer", hint: "Canonical facing W" },
    { id: "top_down", label: "Top-down", hint: "Canonical facing N" },
    { id: "isometric", label: "Isometric", hint: "Canonical facing N" },
    { id: "rts_oblique", label: "RTS oblique", hint: "Side-style facing W" },
  ];

  let {
    busy = false,
    onConfirm,
    onClose,
  }: {
    busy?: boolean;
    onConfirm: (view: string, autoOrient: boolean) => void | Promise<void>;
    onClose: () => void;
  } = $props();

  let selected = $state("side");
  let autoOrient = $state(true);
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <section class="dialog">
    <header>
      <div><p>ANCHOR</p><h2>Game view preset</h2></div>
      <button onclick={onClose}><X size={15} /></button>
    </header>
    <div class="grid">
      {#each presets as preset}
        <button class:selected={selected === preset.id} onclick={() => selected = preset.id}>
          <strong>{preset.label}</strong>
          <small>{preset.hint}</small>
        </button>
      {/each}
    </div>
    <label class="check"><input type="checkbox" bind:checked={autoOrient} /> Auto-orient to canonical facing</label>
    <footer>
      <button onclick={onClose} disabled={busy}>Cancel</button>
      <button class="primary" disabled={busy} onclick={() => onConfirm(selected, autoOrient)}>{busy ? "Promoting…" : "Promote anchor"}</button>
    </footer>
  </section>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:80;background:#000a;display:grid;place-items:center}
  .dialog{width:min(460px,calc(100vw - 30px));background:var(--surface);border:1px solid var(--border-strong);border-radius:10px;padding:18px}
  header{display:flex;justify-content:space-between;align-items:flex-start}
  header p{font-size:9px;letter-spacing:.15em;color:var(--accent);margin:0 0 6px}
  h2{margin:0;font-size:16px}
  header button{border:0;background:transparent;color:var(--faint);cursor:pointer}
  .grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin:16px 0}
  .grid button{display:grid;gap:4px;text-align:left;padding:10px;border:1px solid var(--border);border-radius:6px;background:var(--bg);color:var(--muted);cursor:pointer}
  .grid button.selected{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}
  .grid strong{font-size:11px;color:var(--text)}
  .grid small{font-size:9px;color:var(--faint)}
  .check{display:flex;align-items:center;gap:8px;font-size:10px;color:var(--muted)}
  footer{display:flex;justify-content:flex-end;gap:8px;margin-top:16px}
  footer button{height:30px;border:1px solid var(--border);border-radius:5px;background:var(--bg);color:var(--muted);font:inherit;font-size:11px;padding:0 12px;cursor:pointer}
  footer button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}
  button:disabled{opacity:.45}
</style>
