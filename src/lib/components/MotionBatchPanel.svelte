<script lang="ts">
  import type { BackgroundJob, MotionBatchProgressEntry, MotionPreset } from "$lib/types";

  const categories = ["locomotion", "combat", "reaction", "interaction"] as const;

  let {
    presets,
    selectedIds,
    missingIds = [],
    activeCategory = "locomotion",
    search = "",
    onlyMissing = false,
    busy = false,
    active = false,
    needsConversation = false,
    job,
    entries = [],
    disabled = false,
    onToggle,
    onSelectAllMissing,
    onCategory,
    onSearch,
    onOnlyMissing,
    onQueue,
  }: {
    presets: MotionPreset[];
    selectedIds: string[];
    missingIds?: string[];
    activeCategory?: string;
    search?: string;
    onlyMissing?: boolean;
    busy?: boolean;
    active?: boolean;
    needsConversation?: boolean;
    job?: BackgroundJob;
    entries?: MotionBatchProgressEntry[];
    disabled?: boolean;
    onToggle: (id: string) => void;
    onSelectAllMissing: () => void;
    onCategory: (category: string) => void;
    onSearch: (value: string) => void;
    onOnlyMissing: (value: boolean) => void;
    onQueue: () => void;
  } = $props();

  const filtered = $derived(
    presets.filter(preset => {
      if (preset.category !== activeCategory) return false;
      if (!search.trim()) return true;
      const query = search.trim().toLowerCase();
      return preset.label.toLowerCase().includes(query) || preset.motion.includes(query);
    }),
  );
</script>

<div class="motion-batch">
  <strong>Motion batch</strong>
  <div class="tabs">
    {#each categories as category}
      <button class:active={activeCategory === category} onclick={() => onCategory(category)}>{category}</button>
    {/each}
  </div>
  <input class="search" placeholder="Cerca motion…" value={search} oninput={(event) => onSearch(event.currentTarget.value)} />
  <label class="check"><input type="checkbox" checked={onlyMissing} onchange={(event) => onOnlyMissing(event.currentTarget.checked)} /> Solo mancanti ({missingIds.length})</label>
  <button class="link" type="button" disabled={!missingIds.length} onclick={onSelectAllMissing}>Seleziona tutti i mancanti</button>
  <div class="preset-grid">
    {#each filtered as preset}
      <label class="preset-check" class:missing={missingIds.includes(preset.id)}>
        <input type="checkbox" checked={selectedIds.includes(preset.id)} onchange={() => onToggle(preset.id)} />
        <span>{preset.label}</span>
        <small>{preset.frameCount}f</small>
      </label>
    {/each}
  </div>
  {#if needsConversation}<p class="warn">Motion senza facing canonico richiedono una conversazione chat per la generazione AI.</p>{/if}
  <button class="wide" disabled={disabled || busy || active || !selectedIds.length} onclick={onQueue}>{active ? "Motion batch running…" : "Queue motion batch"}</button>
  {#if active && job}
    <article class="contract-job"><div><strong>{job.stage}</strong><span>{Math.round(job.progress * 100)}%</span></div><i><b style={`width:${job.progress * 100}%`}></b></i></article>
  {/if}
  {#if entries.length}
    <table class="batch-table">
      <thead><tr><th>Motion</th><th>Facing</th><th>Phase</th><th>Status</th></tr></thead>
      <tbody>
        {#each entries as entry}
          <tr class={entry.status}><td>{entry.motion}</td><td>{entry.facing?.toUpperCase() ?? "—"}</td><td>{entry.phase}</td><td>{entry.status}{#if entry.message}<small>{entry.message}</small>{/if}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .motion-batch{display:flex;flex-direction:column;gap:8px;margin-top:10px;border-top:1px solid var(--border);padding-top:10px}
  .tabs{display:flex;flex-wrap:wrap;gap:4px}.tabs button{height:22px;border:1px solid var(--border);background:var(--surface);color:var(--faint);border-radius:4px;font:inherit;font-size:9px;padding:0 6px;cursor:pointer;text-transform:capitalize}.tabs button.active{color:var(--text);border-color:var(--accent)}
  .search{height:26px;border:1px solid var(--border);background:var(--bg);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 8px}
  .check{font-size:10px;color:var(--muted);display:flex;align-items:center;gap:6px}
  .link{border:0;background:transparent;color:var(--accent);font:inherit;font-size:10px;text-align:left;cursor:pointer;padding:0}.link:disabled{opacity:.4;cursor:not-allowed}
  .preset-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:4px}
  .preset-check{display:flex;align-items:center;gap:6px;font-size:10px;color:var(--muted);padding:4px;border-radius:4px;border:1px solid transparent}
  .preset-check.missing{border-color:#c89a4b33}
  .preset-check span{flex:1}.preset-check small{color:var(--faint)}
  .warn{font-size:10px;color:#c89a4b;margin:0}
  .wide{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;font:inherit;font-size:11px;cursor:pointer}
  .contract-job{font-size:10px;color:var(--muted)}.contract-job div{display:flex;justify-content:space-between;margin-bottom:4px}.contract-job i{display:block;height:4px;background:var(--border);border-radius:2px;overflow:hidden}.contract-job b{display:block;height:100%;background:var(--accent)}
  .batch-table{width:100%;border-collapse:collapse;font-size:9px}.batch-table th,.batch-table td{border-bottom:1px solid var(--border);padding:4px 2px;text-align:left}.batch-table small{display:block;color:var(--faint)}
</style>
