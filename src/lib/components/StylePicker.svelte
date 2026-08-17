<script lang="ts">
  import { Check, Layers3 } from "lucide-svelte";
  import { assetUrl } from "$lib/api";
  import { allStylePresets, type ConversationStyleId, type StylePresetId } from "$lib/style-presets";
  import type { CustomArtStyle } from "$lib/library-types";

  let { value, customStyles = [], allowInherit = false, inheritedStyle = "pixel-rpg", compact = false, onChange }: {
    value: ConversationStyleId; customStyles?: CustomArtStyle[]; allowInherit?: boolean; inheritedStyle?: StylePresetId; compact?: boolean;
    onChange: (value: ConversationStyleId) => void | Promise<void>;
  } = $props();
  const styles=$derived(allStylePresets(customStyles));
</script>

<div class="style-grid" class:compact>
  {#if allowInherit}
    <button class="preset inherit" class:selected={value === "inherit"} onclick={() => onChange("inherit")}>
      <span class="inherit-preview"><Layers3 size={20}/></span>
      <span class="copy"><strong>Use project art</strong><small>Currently {styles.find(item => item.id === inheritedStyle)?.name}</small></span>
      {#if value === "inherit"}<span class="check"><Check size={12}/></span>{/if}
    </button>
  {/if}
  {#each styles as preset}
    <button class="preset" class:selected={value === preset.id} onclick={() => onChange(preset.id)}>
      {#if preset.thumbnail}<img src={preset.thumbnail.startsWith("/")?preset.thumbnail:assetUrl(preset.thumbnail)} alt=""/>{:else}<span class="inherit-preview"><Layers3 size={20}/></span>{/if}
      <span class="copy"><strong>{preset.name}</strong><small>{preset.description}</small></span>
      {#if value === preset.id}<span class="check"><Check size={12}/></span>{/if}
    </button>
  {/each}
</div>

<style>
  .style-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px;margin-top:18px}.preset{position:relative;min-width:0;border:1px solid var(--border);background:var(--surface);color:var(--text);border-radius:9px;padding:0;overflow:hidden;text-align:left;cursor:pointer}.preset:hover{border-color:var(--border-strong);background:var(--surface-hover)}.preset.selected{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.preset img,.inherit-preview{width:100%;height:92px;display:grid;place-items:center;object-fit:cover;background:var(--preview)}.inherit-preview{color:var(--faint)}.copy{display:block;padding:10px 11px 12px}.copy strong,.copy small{display:block}.copy strong{font-size:13px;font-weight:620}.copy small{font-size:11px;line-height:1.35;color:var(--muted);margin-top:4px}.check{position:absolute;top:7px;right:7px;width:22px;height:22px;border-radius:50%;background:var(--accent);color:white;display:grid;place-items:center;box-shadow:0 2px 8px #0006}.style-grid.compact{grid-template-columns:1fr;gap:7px;margin-top:10px}.compact .preset{height:62px;display:grid;grid-template-columns:76px minmax(0,1fr)}.compact .preset img,.compact .inherit-preview{height:62px}.compact .copy{padding:9px 28px 8px 10px}.compact .copy strong{font-size:12px}.compact .copy small{font-size:10px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.compact .check{top:20px;right:8px}@media(max-width:720px){.style-grid{grid-template-columns:1fr}.preset{display:grid;grid-template-columns:110px minmax(0,1fr)}.preset img,.inherit-preview{height:76px}.copy{padding:12px}}
</style>
