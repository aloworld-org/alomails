// The AI-provider catalog the admin screen presents. Every kind here speaks the
// OpenAI-compatible Chat Completions contract our inference layer drives:
// self-hosted Ollama, the built-in alo AI (point it at your EU-hosted or
// local endpoint), OpenAI, Anthropic (via its OpenAI-compatible endpoint), and
// a custom endpoint.
import { strings } from "../i18n";

export interface CatalogEntry {
  kind: string;
  name: string;
  description: string;
  group: "self" | "keys";
  defaultBaseUrl: string;
  needsKey: boolean;
  /** A sensible model name to prefill the "add model" hint. */
  defaultModel?: string;
  /** Shown with a "Built in" tag. */
  builtIn?: boolean;
}

export const CATALOG: CatalogEntry[] = [
  {
    kind: "ollama",
    name: strings.kindOllama,
    description: strings.ollamaDesc,
    group: "self",
    defaultBaseUrl: "http://localhost:11434",
    needsKey: false,
    defaultModel: "llama3.2",
  },
  {
    kind: "alo",
    name: strings.kindalo,
    description: strings.aloDesc,
    group: "self",
    defaultBaseUrl: "",
    needsKey: false,
    builtIn: true,
  },
  // EU-hosted first among the key-based providers — the sovereignty-aligned pick.
  {
    kind: "mistral",
    name: strings.kindMistral,
    description: strings.mistralDesc,
    group: "keys",
    defaultBaseUrl: "https://api.mistral.ai",
    needsKey: true,
    defaultModel: "mistral-small-latest",
  },
  {
    kind: "openai",
    name: strings.kindOpenai,
    description: strings.openaiDesc,
    group: "keys",
    defaultBaseUrl: "https://api.openai.com",
    needsKey: true,
    defaultModel: "gpt-4o-mini",
  },
  {
    kind: "anthropic",
    name: strings.kindAnthropic,
    description: strings.anthropicDesc,
    group: "keys",
    defaultBaseUrl: "https://api.anthropic.com",
    needsKey: true,
    defaultModel: "claude-3-5-sonnet-latest",
  },
  {
    kind: "custom",
    name: strings.kindCustom,
    description: strings.customDesc,
    group: "keys",
    defaultBaseUrl: "",
    needsKey: true,
  },
];
