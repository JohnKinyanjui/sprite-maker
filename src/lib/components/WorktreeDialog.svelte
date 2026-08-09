<script lang="ts">
  import { X, Plus, UserRound, Trees, PawPrint, Package, Grid3X3, Clapperboard, Sparkles, PanelsTopLeft, FolderTree, ArrowRight } from "lucide-svelte";
  import type { WorktreeKind } from "$lib/types";

  let { busy = false, onCreate, onClose }: {
    busy?: boolean;
    onCreate: (name: string, kind: WorktreeKind, description?: string) => void | Promise<void>;
    onClose: () => void;
  } = $props();
  let name = $state("");
  let kind = $state<WorktreeKind>("character");
  let description = $state("");

  const kinds = [
    { id: "character" as const, label: "Character", icon: UserRound, detail: "One playable character or NPC", example: "Knight, farmer, shopkeeper" },
    { id: "creature" as const, label: "Creature", icon: PawPrint, detail: "A monster or animal family", example: "Cave centipede, slime, wolf" },
    { id: "environment" as const, label: "Environment", icon: Trees, detail: "A place and its world art", example: "Forest, dungeon, desert" },
    { id: "object" as const, label: "Game object", icon: Package, detail: "An interactive object or prop", example: "Chest, door, turret, pickup" },
    { id: "tileset" as const, label: "Tileset", icon: Grid3X3, detail: "Connected terrain and edge rules", example: "Village ground, cave walls" },
    { id: "animation" as const, label: "Motion library", icon: Clapperboard, detail: "Reusable motion studies", example: "Sword attacks, locomotion" },
    { id: "vfx" as const, label: "VFX", icon: Sparkles, detail: "Transparent animated effects", example: "Fire magic, impacts, smoke" },
    { id: "ui" as const, label: "Game UI", icon: PanelsTopLeft, detail: "Interface art and animation", example: "HUD, icons, cursors" },
  ];
  let selectedKind = $derived(kinds.find(item=>item.id===kind)??kinds[0]);

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!name.trim() || busy) return;
    onCreate(name.trim(), kind, description.trim() || undefined);
  }
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <form class="dialog" onsubmit={submit}>
    <header><div><p>NEW WORKTREE</p><h2>What are you making?</h2><span>A worktree is one focused production area inside your game project.</span></div><button type="button" onclick={onClose} title="Close"><X size={15}/></button></header>
    <div class="map"><span><FolderTree size={13}/>Project</span><ArrowRight size={12}/><span class="active">Worktree</span><ArrowRight size={12}/><span>Chats · references · sprites · animations · exports</span></div>
    <div class="kind-label">CHOOSE WHAT THIS AREA OWNS</div>
    <div class="kinds">{#each kinds as item}{@const Icon=item.icon}<button type="button" class:selected={kind===item.id} onclick={()=>kind=item.id}><Icon size={16}/><span><strong>{item.label}</strong><small>{item.detail}</small></span></button>{/each}</div>
    <div class="fields"><label>Name<input bind:value={name} placeholder={selectedKind.example.split(",")[0]}/><small>Examples: {selectedKind.example}</small></label><label>Goal or constraints <em>optional</em><textarea bind:value={description} rows="2" placeholder="Style, dimensions, gameplay role, or anything this worktree should remember"></textarea></label></div>
    <div class="result"><strong>{name.trim()||selectedKind.label}</strong><span>{selectedKind.label} worktree</span><small>New chats and generated assets will be scoped here until you switch worktrees.</small></div>
    <footer><button type="button" onclick={onClose}>Cancel</button><button class="primary" disabled={!name.trim() || busy}><Plus size={13}/>{busy ? "Creating…" : `Create ${selectedKind.label}`}</button></footer>
  </form>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:48;background:#000a;display:grid;place-items:center}.dialog{width:min(660px,calc(100vw - 32px));max-height:calc(100vh - 42px);overflow:auto;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 28px 80px #000a;padding:21px}.dialog header{display:flex;align-items:flex-start;justify-content:space-between;margin-bottom:15px}.dialog header p{font-size:9px;letter-spacing:.14em;color:var(--accent);font-weight:700;margin:0 0 6px}.dialog h2{font-size:18px;margin:0}.dialog header span{display:block;font-size:11px;color:var(--faint);margin-top:6px}.dialog header button{width:27px;height:27px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:5px;cursor:pointer}.dialog header button:hover{background:var(--surface-hover);color:var(--text)}.map{height:34px;border:1px solid var(--border);border-radius:6px;background:var(--bg);display:flex;align-items:center;gap:7px;padding:0 9px;color:var(--faint);font-size:9px}.map span{display:flex;align-items:center;gap:4px}.map .active{color:var(--accent);font-weight:650}.kind-label{font-size:9px;letter-spacing:.13em;font-weight:700;color:var(--faint);margin:16px 0 7px}.kinds{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px}.kinds button{min-height:52px;border:1px solid var(--border);border-radius:7px;background:var(--bg);color:var(--faint);display:grid;grid-template-columns:20px minmax(0,1fr);align-items:center;gap:7px;padding:7px 9px;text-align:left;cursor:pointer}.kinds button:hover{border-color:var(--border-strong);background:var(--surface-hover)}.kinds button.selected{border-color:var(--accent);background:var(--accent-dim);color:var(--accent)}.kinds span,.kinds strong,.kinds small{display:block;min-width:0}.kinds strong{font-size:11px;color:var(--text)}.kinds small{font-size:9px;color:var(--faint);margin-top:3px}.fields{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-top:14px}label{display:block;font-size:10px;color:var(--muted)}label em{font-style:normal;color:var(--faint)}input,textarea{display:block;width:100%;box-sizing:border-box;margin-top:6px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--text);font:inherit;font-size:11px;outline:0}input{height:35px;padding:0 9px}textarea{resize:vertical;padding:8px 9px;line-height:1.4}input:focus,textarea:focus{border-color:var(--accent)}label>small{display:block;color:var(--faint);font-size:8px;margin-top:5px}.result{margin-top:13px;border-left:2px solid var(--accent);background:var(--bg);padding:8px 10px;display:grid;grid-template-columns:auto 1fr;gap:2px 7px}.result strong{font-size:10px}.result span{font-size:9px;color:var(--accent)}.result small{grid-column:1/3;font-size:8px;color:var(--faint)}footer{display:flex;justify-content:flex-end;gap:7px;border-top:1px solid var(--border);padding-top:15px;margin-top:15px}footer button{height:32px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--muted);font:inherit;font-size:10px;padding:0 11px;display:flex;align-items:center;gap:6px;cursor:pointer}footer .primary{background:var(--text);border-color:var(--text);color:var(--bg)}button:disabled{opacity:.45;cursor:not-allowed}@media(max-width:620px){.kinds,.fields{grid-template-columns:1fr}.map span:last-child{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
</style>
