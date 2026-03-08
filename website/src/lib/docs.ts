import { readFile, readdir } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

export type DocSection = {
  title: string;
  body: string;
};

export type DocItem = {
  slug: string;
  key: string;
  label: string;
  group: "start" | "core" | "planning" | "legal" | "other";
  lines: number;
  sections: DocSection[];
};

const thisDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(thisDir, "../../../");
const docsDir = path.join(root, "docs");

const staticFiles = [
  { key: "README.md", label: "README", abs: path.join(root, "README.md") },
  { key: "CONTRIBUTING.md", label: "Contributing", abs: path.join(root, "CONTRIBUTING.md") },
  { key: "LICENSE", label: "License", abs: path.join(root, "LICENSE") },
];

const normalize = (s: string) => s.replace(/\r\n/g, "\n").trim();

const parseSections = (rawText: string): DocSection[] => {
  const text = normalize(rawText);
  if (!text) return [{ title: "Overview", body: "No content found." }];

  const lines = text.split("\n");
  const sections: DocSection[] = [];
  let currentTitle = "Overview";
  let currentBody: string[] = [];

  const pushCurrent = () => {
    sections.push({
      title: currentTitle,
      body: currentBody.join("\n").trim() || "(No details in this section)",
    });
  };

  for (const line of lines) {
    const h = line.match(/^##?\s+(.*)$/);
    if (h) {
      pushCurrent();
      currentTitle = h[1].trim();
      currentBody = [];
      continue;
    }
    currentBody.push(line);
  }
  pushCurrent();

  if (sections[0]?.title === "Overview" && sections[0]?.body === "(No details in this section)") {
    sections.shift();
  }
  return sections.length ? sections : [{ title: "Overview", body: text }];
};

const classifyGroup = (key: string): DocItem["group"] => {
  const k = key.toLowerCase();
  if (k.endsWith("readme.md") || k.endsWith("install-windows.md") || k.endsWith("contributing.md")) return "start";
  if (k.endsWith("architecture.md") || k.endsWith("benchmarks.md")) return "core";
  if (k.endsWith("roadmap.md") || k.endsWith("issue-seeds.md")) return "planning";
  if (k.endsWith("license")) return "legal";
  return "other";
};

const toSlug = (key: string) =>
  key
    .toLowerCase()
    .replace(/^docs\//, "")
    .replace(/\.(md)$/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

export async function loadDocs(): Promise<DocItem[]> {
  const docsFiles = (await readdir(docsDir))
    .filter((name) => name.toLowerCase().endsWith(".md"))
    .sort((a, b) => a.localeCompare(b))
    .map((name) => ({
      key: `docs/${name}`,
      label: name.replace(".md", ""),
      abs: path.join(docsDir, name),
    }));

  const allFiles = [...staticFiles, ...docsFiles];

  const docs = await Promise.all(
    allFiles.map(async (item) => {
      let raw = "";
      try {
        raw = await readFile(item.abs, "utf8");
      } catch {
        raw = "";
      }
      const sections = parseSections(raw);
      return {
        slug: toSlug(item.key),
        key: item.key,
        label: item.label,
        group: classifyGroup(item.key),
        lines: raw ? raw.split(/\r?\n/).length : 0,
        sections,
      } as DocItem;
    })
  );

  return docs;
}

export function groupedDocs(docs: DocItem[]) {
  return {
    start: docs.filter((d) => d.group === "start"),
    core: docs.filter((d) => d.group === "core"),
    planning: docs.filter((d) => d.group === "planning"),
    legal: docs.filter((d) => d.group === "legal"),
    other: docs.filter((d) => d.group === "other"),
  };
}
