<script lang="ts">
  import DOMPurify from "dompurify";
  import { marked } from "marked";
  import { browser } from "$app/environment";
  import { isAbsolute, normalize, resolve, sep } from "@tauri-apps/api/path";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { classifyMarkdownLink, pathIsInsideWorkspace } from "$lib/markdown-links";

  let { content, workspacePath, onLinkError = () => {} }: { content: string; workspacePath: string; onLinkError?: (message: string) => void } = $props();

  function renderMarkdown(value: string) {
    // Agent output is untrusted. Escape embedded HTML before Markdown parsing,
    // then sanitize the generated markup again in the webview.
    const escaped = value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
    const rendered = marked.parse(escaped, { gfm: true, breaks: true, async: false }) as string;
    if (!browser) return rendered.replace(/href=(['"])\s*(?:javascript|data):.*?\1/gi, 'href="#"');
    return DOMPurify.sanitize(rendered, {
      ALLOWED_TAGS: ["p","br","strong","em","del","code","pre","blockquote","ul","ol","li","a","h1","h2","h3","h4","hr","table","thead","tbody","tr","th","td"],
      ALLOWED_ATTR: ["href","title"],
      ALLOW_UNKNOWN_PROTOCOLS: false,
    });
  }

  async function followLink(event: MouseEvent) {
    const target = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a") : undefined;
    if (!target) return;
    event.preventDefault();
    event.stopPropagation();

    const link = classifyMarkdownLink(target.getAttribute("href"));
    if (link.kind === "ignore") return;
    if (link.kind === "blocked") { onLinkError(link.reason); return; }

    try {
      if (link.kind === "external") { await openUrl(link.url); return; }
      const root = await normalize(workspacePath);
      const candidate = await normalize(await isAbsolute(link.path) ? link.path : await resolve(root, link.path));
      if (!pathIsInsideWorkspace(root, candidate, sep())) {
        onLinkError("This link points outside the active workspace and was not opened.");
        return;
      }
      await revealItemInDir(candidate);
    } catch {
      onLinkError("This workspace file or folder is no longer available.");
    }
  }

  function interceptLinks(node: HTMLElement) {
    node.addEventListener("click", followLink);
    return { destroy: () => node.removeEventListener("click", followLink) };
  }

  let html = $derived(renderMarkdown(content));
</script>

<div class="markdown" use:interceptLinks>{@html html}</div>

<style>
  .markdown{font-size:16px;line-height:1.62;color:var(--text);overflow-wrap:anywhere}.markdown :global(p){margin:0 0 .72em}.markdown :global(p:last-child){margin-bottom:0}.markdown :global(h1),.markdown :global(h2),.markdown :global(h3),.markdown :global(h4){line-height:1.3;margin:1.2em 0 .45em;font-weight:680}.markdown :global(h1){font-size:1.35em}.markdown :global(h2){font-size:1.2em}.markdown :global(h3),.markdown :global(h4){font-size:1.05em}.markdown :global(ul),.markdown :global(ol){margin:.45em 0 .8em;padding-left:1.55em}.markdown :global(li){margin:.18em 0}.markdown :global(code){font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.88em;padding:.12em .32em;border:1px solid var(--border);border-radius:4px;background:var(--surface)}.markdown :global(pre){overflow:auto;padding:12px;border:1px solid var(--border);border-radius:7px;background:var(--sidebar)}.markdown :global(pre code){padding:0;border:0;background:transparent}.markdown :global(blockquote){margin:.7em 0;padding:.1em 0 .1em 12px;border-left:2px solid var(--accent);color:var(--muted)}.markdown :global(a){color:#a98df3;text-decoration:none}.markdown :global(a:hover){text-decoration:underline}.markdown :global(table){border-collapse:collapse;margin:.8em 0;font-size:.9em}.markdown :global(th),.markdown :global(td){border:1px solid var(--border);padding:5px 8px;text-align:left}.markdown :global(hr){border:0;border-top:1px solid var(--border);margin:1em 0}
</style>
