<script lang="ts">
  import { FolderPlus, Sparkles } from "lucide-svelte";
  let { onAdd }:{onAdd:()=>void}=$props();
</script>
<section><div class="mark"><Sparkles size={22}/></div><h1>Start a Sprite Studio project</h1><p>Projects hold your chats, media, animation work, skills, and art direction. Add a local folder to begin.</p><button onclick={onAdd}><FolderPlus size={15}/>Add project</button></section>
<style>
  section{height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;background:var(--bg);padding:32px}.mark{width:44px;height:44px;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);color:var(--accent);display:grid;place-items:center}h1{font-size:24px;letter-spacing:-.035em;margin:18px 0 8px}p{max-width:430px;color:var(--muted);font-size:12px;line-height:1.6;margin:0 0 20px}button{height:37px;border:1px solid var(--accent);border-radius:7px;background:var(--accent);color:#111;display:flex;align-items:center;gap:7px;padding:0 13px;font:inherit;font-size:11px;font-weight:650;cursor:pointer}
</style>
