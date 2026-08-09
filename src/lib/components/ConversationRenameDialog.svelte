<script lang="ts">
  import { onMount } from "svelte";
  import { Pencil, X } from "lucide-svelte";
  import type { Conversation } from "$lib/types";

  let { conversation, busy = false, onRename, onClose }: {
    conversation: Conversation;
    busy?: boolean;
    onRename: (title: string) => void | Promise<void>;
    onClose: () => void;
  } = $props();
  let title = $state("");
  let input: HTMLInputElement;

  onMount(() => {
    title = conversation.title;
    requestAnimationFrame(() => { input.focus(); input.select(); });
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!title.trim() || busy) return;
    onRename(title.trim());
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={keydown}/>

<div class="backdrop" role="presentation" onclick={(event)=>event.target===event.currentTarget&&onClose()}>
  <form class="dialog" onsubmit={submit} aria-labelledby="rename-chat-title">
    <header>
      <div><p>CHAT</p><h2 id="rename-chat-title">Rename chat</h2><span>Use a short title that makes this task easy to find.</span></div>
      <button type="button" onclick={onClose} title="Close" aria-label="Close rename dialog"><X size={16}/></button>
    </header>
    <label for="chat-title">Chat title</label>
    <input id="chat-title" bind:this={input} bind:value={title} maxlength="80" autocomplete="off"/>
    <footer>
      <button type="button" onclick={onClose}>Cancel</button>
      <button class="primary" disabled={!title.trim() || busy}><Pencil size={13}/>{busy ? "Renaming…" : "Rename"}</button>
    </footer>
  </form>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:52;background:#000a;display:grid;place-items:center}.dialog{width:min(420px,calc(100vw - 32px));border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);box-shadow:0 28px 80px #000a;padding:21px}.dialog header{display:flex;align-items:flex-start;justify-content:space-between}.dialog header p{font-size:10px;letter-spacing:.13em;color:var(--accent);font-weight:700;margin:0 0 7px}.dialog h2{font-size:19px;line-height:1.2;margin:0}.dialog header span{display:block;font-size:12px;color:var(--faint);margin-top:7px}.dialog header button{width:29px;height:29px;border:0;background:transparent;color:var(--faint);display:grid;place-items:center;border-radius:6px;cursor:pointer}.dialog header button:hover{background:var(--surface-hover);color:var(--text)}label{display:block;margin-top:24px;color:var(--muted);font-size:12px}input{display:block;width:100%;height:39px;margin-top:7px;border:1px solid var(--border-strong);border-radius:7px;background:var(--bg);color:var(--text);font:inherit;font-size:14px;padding:0 11px;outline:0}input:focus{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-dim)}footer{display:flex;justify-content:flex-end;gap:8px;border-top:1px solid var(--border);padding-top:16px;margin-top:20px}footer button{height:34px;border:1px solid var(--border-strong);border-radius:7px;background:var(--bg);color:var(--muted);font:inherit;font-size:12px;padding:0 13px;display:flex;align-items:center;gap:6px;cursor:pointer}footer .primary{background:var(--text);border-color:var(--text);color:var(--bg);font-weight:620}button:disabled{opacity:.45;cursor:not-allowed}
</style>
