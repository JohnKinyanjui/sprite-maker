<script lang="ts">
  import { MessageSquare, Images, BookImage, Clapperboard, Grid3X3, Gamepad2, Keyboard, Sparkles, Boxes } from "lucide-svelte";

  let { active, conversationTitle, worktreeName, assetCount, referenceCount, animationCount, packCount = 0, showVfx = false, onSelect }: {
    active: string; conversationTitle?: string; worktreeName?: string; assetCount: number; referenceCount: number; animationCount: number; packCount?: number; showVfx?: boolean;
    onSelect: (tab: string) => void;
  } = $props();

  const tabs = $derived([
    { id: "chat", label: conversationTitle || "Chat", icon: MessageSquare, meta: "" },
    { id: "sprites", label: "Sprites", icon: Images, meta: String(assetCount) },
    { id: "references", label: "References", icon: BookImage, meta: String(referenceCount) },
    { id: "animate", label: "Animate", icon: Clapperboard, meta: String(animationCount) },
    ...(showVfx?[{ id: "vfx", label: "VFX", icon: Sparkles, meta: "" }]:[]),
    { id: "sheets", label: "Sheets", icon: Grid3X3, meta: "" },
    { id: "packs", label: "Packs", icon: Boxes, meta: String(packCount) },
    { id: "play", label: "Playground", icon: Gamepad2, meta: "" },
  ]);
</script>

<nav class="tabs" aria-label="Workspace tools">
  <div class="tab-list">
    {#each tabs as tab, index}
      {@const Icon = tab.icon}
      <button class:active={active === tab.id} onclick={() => onSelect(tab.id)} title={`${tab.label} · Ctrl/⌘+${index + 1}`}>
        <Icon size={13}/><span>{tab.label}</span>{#if tab.meta}<small>{tab.meta}</small>{/if}
      </button>
    {/each}
  </div>
  <div class="shortcut"><strong>{worktreeName ?? "Project"}</strong><span>·</span><Keyboard size={12}/><span>Ctrl/⌘ 1–{tabs.length}</span></div>
</nav>

<style>
  .tabs{height:44px;min-height:44px;border-bottom:1px solid var(--border);background:var(--sidebar);display:flex;align-items:flex-end;justify-content:space-between;padding:0 11px;user-select:none}.tab-list{height:100%;display:flex;align-items:flex-end;gap:3px;min-width:0}.tab-list button{position:relative;height:37px;min-width:96px;max-width:220px;border:1px solid transparent;border-bottom:0;background:transparent;color:var(--faint);border-radius:7px 7px 0 0;padding:0 10px;display:flex;align-items:center;gap:7px;font:inherit;font-size:12px;cursor:pointer}.tab-list button:hover{color:var(--text);background:var(--surface-hover)}.tab-list button.active{height:38px;color:var(--text);background:var(--bg);border-color:var(--border)}.tab-list button.active:after{content:"";position:absolute;left:0;right:0;bottom:-1px;height:2px;background:var(--bg)}.tab-list span{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.tab-list small{margin-left:auto;min-width:19px;height:19px;border-radius:10px;background:var(--selected);color:var(--muted);display:grid;place-items:center;font-size:12px}.shortcut{height:37px;display:flex;align-items:center;gap:6px;color:var(--faint);font-size:11px;padding:0 5px}.shortcut strong{max-width:130px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--muted);font-weight:600}@media(max-width:1050px){.tab-list button{min-width:84px;padding:0 8px}.shortcut{display:none}}
</style>
