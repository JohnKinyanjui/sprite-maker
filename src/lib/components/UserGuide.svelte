<script lang="ts">
  import DOMPurify from "dompurify";
  import { marked } from "marked";
  import { browser } from "$app/environment";
  import { BookOpen } from "lucide-svelte";
  import guideEn from "../../../docs/en/user-guide.md?raw";
  import guideIt from "../../../docs/it/guida-utente.md?raw";

  type GuideLanguage = "en" | "it";

  let language = $state<GuideLanguage>("it");
  let activeSection = $state("");

  const sources: Record<GuideLanguage, string> = { en: guideEn, it: guideIt };

  function renderMarkdown(value: string) {
    const escaped = value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
    const rendered = marked.parse(escaped, { gfm: true, breaks: true, async: false }) as string;
    if (!browser) return rendered;
    return DOMPurify.sanitize(rendered, {
      ALLOWED_TAGS: ["p", "br", "strong", "em", "del", "code", "pre", "blockquote", "ul", "ol", "li", "a", "h1", "h2", "h3", "h4", "hr", "table", "thead", "tbody", "tr", "th", "td"],
      ALLOWED_ATTR: ["href", "title", "id"],
      ALLOW_UNKNOWN_PROTOCOLS: false,
    });
  }

  const sections = $derived(
    [...sources[language].matchAll(/^## (.+)$/gm)].map((match, index) => ({
      id: `section-${index}`,
      title: match[1].replace(/—.*/, "").trim(),
    })),
  );

  function scrollToSection(id: string) {
    activeSection = id;
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

<section class="guide">
  <header>
    <div>
      <span><BookOpen size={12}/> DOCUMENTATION</span>
      <h1>{language === "it" ? "Guida utente" : "User guide"}</h1>
      <p>{language === "it" ? "Best practice, prompt efficaci e pipeline di produzione — consultabile anche offline." : "Best practices, effective prompts, and production pipeline — available offline in the app."}</p>
    </div>
    <div class="lang" role="group" aria-label="Guide language">
      <button class:active={language === "it"} onclick={() => (language = "it")}>Italiano</button>
      <button class:active={language === "en"} onclick={() => (language = "en")}>English</button>
    </div>
  </header>
  <div class="layout">
    <nav aria-label="Guide sections">
      <div class="nav-label">{language === "it" ? "INDICE" : "CONTENTS"}</div>
      {#each sections as section, index}
        <button class:active={activeSection === section.id} onclick={() => scrollToSection(section.id)}>{section.title}</button>
      {/each}
    </nav>
    <article class="content">
      {#each sources[language].split(/^## /gm).filter(Boolean) as block, index}
        {@const title = block.split("\n")[0]?.trim() ?? ""}
        <section id={`section-${index}`}>
          <div class="markdown">{@html renderMarkdown(`## ${block}`)}</div>
        </section>
      {/each}
    </article>
  </div>
</section>

<style>
  .guide{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}.guide>header{display:flex;align-items:flex-start;justify-content:space-between;gap:24px;padding:34px clamp(28px,5vw,72px) 22px;border-bottom:1px solid var(--border)}.guide>header span{display:flex;align-items:center;gap:6px;font-size:9px;letter-spacing:.15em;font-weight:700;color:var(--accent)}.guide h1{font-size:28px;letter-spacing:-.035em;margin:8px 0}.guide>header p{font-size:12px;line-height:1.55;color:var(--muted);margin:0;max-width:620px}.lang{display:flex;gap:4px;padding-top:8px}.lang button{height:32px;border:1px solid var(--border);border-radius:6px;background:var(--surface);color:var(--muted);padding:0 12px;font:inherit;font-size:11px;cursor:pointer}.lang button.active,.lang button:hover{border-color:var(--accent);color:var(--text)}.lang button.active{background:var(--accent-dim)}.layout{min-height:0;flex:1;display:grid;grid-template-columns:220px minmax(0,1fr);overflow:hidden}nav{border-right:1px solid var(--border);background:var(--sidebar);padding:16px 10px;overflow:auto}.nav-label{font-size:10px;letter-spacing:.13em;font-weight:700;color:var(--faint);padding:4px 8px 10px}nav button{width:100%;border:0;background:transparent;color:var(--muted);border-radius:5px;text-align:left;padding:7px 8px;font:inherit;font-size:11px;line-height:1.4;cursor:pointer}nav button:hover,nav button.active{background:var(--selected);color:var(--text)}.content{overflow:auto;padding:28px clamp(28px,5vw,72px) 48px}.content>section{margin-bottom:28px;padding-bottom:8px;border-bottom:1px solid var(--border)}.content>section:last-child{border-bottom:0}.markdown :global(h1){display:none}.markdown :global(h2){font-size:18px;margin:0 0 12px;scroll-margin-top:18px}.markdown :global(h3){font-size:13px;margin:18px 0 8px}.markdown :global(p),.markdown :global(li){font-size:12px;line-height:1.6;color:var(--muted)}.markdown :global(strong){color:var(--text)}.markdown :global(code){font-size:11px;background:var(--surface);border:1px solid var(--border);border-radius:4px;padding:1px 5px}.markdown :global(pre){overflow:auto;padding:12px;border:1px solid var(--border);border-radius:6px;background:var(--sidebar)}.markdown :global(table){width:100%;border-collapse:collapse;font-size:11px;margin:12px 0}.markdown :global(th),.markdown :global(td){border:1px solid var(--border);padding:7px 9px;text-align:left;vertical-align:top}.markdown :global(th){background:var(--sidebar);color:var(--text)}.markdown :global(hr){border:0;border-top:1px solid var(--border);margin:20px 0}.markdown :global(a){color:var(--accent)}@media(max-width:900px){.layout{grid-template-columns:1fr}nav{display:none}}
</style>
