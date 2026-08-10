export type MarkdownLinkTarget =
  | { kind: "external"; url: string }
  | { kind: "workspace"; path: string }
  | { kind: "ignore" }
  | { kind: "blocked"; reason: string };

const WINDOWS_ABSOLUTE_PATH = /^[a-z]:[\\/]/i;
const URL_SCHEME = /^[a-z][a-z\d+.-]*:/i;
const EXTERNAL_SCHEMES = /^(?:https?|mailto):/i;

function decoded(value: string): string {
  try { return decodeURIComponent(value); } catch { return value; }
}

function fileUrlPath(value: string): string | undefined {
  try {
    const url = new URL(value);
    if (url.protocol !== "file:") return;
    const pathname = decoded(url.pathname);
    return /^\/[a-z]:\//i.test(pathname) ? pathname.slice(1) : pathname;
  } catch {
    return;
  }
}

export function classifyMarkdownLink(href: string | null | undefined): MarkdownLinkTarget {
  const value = href?.trim();
  if (!value || value.startsWith("#")) return { kind: "ignore" };
  if (value.includes("\0")) return { kind: "blocked", reason: "The link contains an invalid path." };
  if (value.startsWith("//")) return { kind: "external", url: `https:${value}` };
  if (EXTERNAL_SCHEMES.test(value)) return { kind: "external", url: value };
  if (/^file:/i.test(value)) {
    const path = fileUrlPath(value);
    return path ? { kind: "workspace", path } : { kind: "blocked", reason: "The file link is invalid." };
  }
  if (WINDOWS_ABSOLUTE_PATH.test(value)) return { kind: "workspace", path: decoded(value) };
  if (URL_SCHEME.test(value)) return { kind: "blocked", reason: "This link type is not supported." };
  return { kind: "workspace", path: decoded(value) };
}

export function pathIsInsideWorkspace(workspacePath: string, candidatePath: string, separator: string): boolean {
  const windows = separator === "\\";
  const normalize = (value: string) => {
    const unified = value.replace(/[\\/]+/g, separator);
    const withoutTrailing = unified.length > 1 ? unified.replace(/[\\/]+$/, "") : unified;
    return windows ? withoutTrailing.toLowerCase() : withoutTrailing;
  };
  const root = normalize(workspacePath);
  const candidate = normalize(candidatePath);
  return candidate === root || candidate.startsWith(`${root}${separator}`);
}
