<script lang="ts">
  import { Plus, X } from "lucide-svelte";
  import type { WorktreeKind } from "$lib/types";

  let { busy = false, onCreate, onClose }: {
    busy?: boolean;
    onCreate: (name: string, kind: WorktreeKind) => void | Promise<void>;
    onClose: () => void;
  } = $props();

  let name = $state("");

  function inferKind(value: string): WorktreeKind {
    const normalized = value.toLowerCase();
    if (/\b(vfx|fx|effect|effects|magic|spell)\b/.test(normalized)) return "vfx";
    if (/\b(character|hero|player|npc|ranger|knight|warrior|mage)\b/.test(normalized)) return "character";
    if (/\b(creature|monster|enemy|animal|centipede|slime|boss)\b/.test(normalized)) return "creature";
    if (/\b(environment|world|forest|dungeon|cave|village|biome|scene)\b/.test(normalized)) return "environment";
    if (/\b(tile|tiles|tileset|terrain)\b/.test(normalized)) return "tileset";
    if (/\b(animation|animations|motion|moveset)\b/.test(normalized)) return "animation";
    if (/\b(ui|hud|interface|menu|icons?)\b/.test(normalized)) return "ui";
    return "object";
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const value = name.trim();
    if (!value || busy) return;
    onCreate(value, inferKind(value));
  }
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
  <form class="dialog" onsubmit={submit}>
    <header>
      <div><p>NEW WORKTREE</p><h2>Create a worktree</h2></div>
      <button type="button" onclick={onClose} title="Close"><X size={15}/></button>
    </header>
    <label>
      Name
      <input bind:value={name} placeholder="e.g. Cave Centipede"/>
    </label>
    <footer>
      <button type="button" onclick={onClose}>Cancel</button>
      <button class="primary" disabled={!name.trim() || busy}><Plus size={13}/>{busy ? "Creating…" : "Create worktree"}</button>
    </footer>
  </form>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:48;background:#000a;display:grid;place-items:center}.dialog{width:min(410px,calc(100vw - 32px));border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 28px 80px #000a;padding:21px}.dialog header{display:flex;align-items:flex-start;justify-content:space-between;margin-bottom:22px}.dialog header p{font-size:9px;letter-spacing:.14em;color:var(--accent);font-weight:700;margin:0 0 6px}.dialog h2{font-size:18px;margin:0}.dialog header button{width:27px;height:27px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:5px;cursor:pointer}.dialog header button:hover{background:var(--surface-hover);color:var(--text)}label{display:block;font-size:10px;color:var(--muted)}input{display:block;width:100%;height:40px;box-sizing:border-box;margin-top:7px;padding:0 11px;border:1px solid var(--border-strong);border-radius:7px;background:var(--bg);color:var(--text);font:inherit;font-size:13px;outline:0}input:focus{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}footer{display:flex;justify-content:flex-end;gap:7px;border-top:1px solid var(--border);padding-top:16px;margin-top:22px}footer button{height:33px;border:1px solid var(--border-strong);border-radius:6px;background:var(--bg);color:var(--muted);font:inherit;font-size:10px;padding:0 12px;display:flex;align-items:center;gap:6px;cursor:pointer}footer .primary{background:var(--text);border-color:var(--text);color:var(--bg)}button:disabled{opacity:.45;cursor:not-allowed}
</style>
