import { MAX_LOGO_BYTES, type BrandLogo } from "./model";

export const ACCEPTED_LOGO_TYPES = ["image/png", "image/jpeg", "image/webp", "image/svg+xml"] as const;
export type LogoFileError = "unsupported" | "too-large";

export function validateLogoFile(file: File): LogoFileError | null {
  if (!ACCEPTED_LOGO_TYPES.includes(file.type as (typeof ACCEPTED_LOGO_TYPES)[number])) return "unsupported";
  if (file.size > MAX_LOGO_BYTES) return "too-large";
  return null;
}

export async function readLogoFile(file: File, id: string): Promise<BrandLogo> {
  if (file.type === "image/svg+xml" && !isSafeSvg(await readFileText(file))) {
    throw new Error("unsafe-svg");
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("logo-read-failed"));
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        reject(new Error("logo-read-failed"));
        return;
      }
      resolve({
        id,
        name: file.name,
        label: file.name.replace(/\.[^.]+$/, "").trim().slice(0, 48),
        mimeType: file.type as BrandLogo["mimeType"],
        dataUrl: reader.result,
      });
    };
    reader.readAsDataURL(file);
  });
}

function readFileText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("logo-read-failed"));
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("logo-read-failed"));
    reader.readAsText(file);
  });
}

function isSafeSvg(markup: string): boolean {
  const document = new DOMParser().parseFromString(markup, "image/svg+xml");
  if (document.querySelector("parsererror") !== null || document.documentElement.tagName.toLowerCase() !== "svg") return false;
  if (document.querySelector("script, style, foreignObject, iframe, object, embed, audio, video") !== null) return false;
  return Array.from(document.querySelectorAll("*")).every((element) =>
    Array.from(element.attributes).every((attribute) => {
      const name = attribute.name.toLowerCase();
      const value = attribute.value.trim().toLowerCase();
      if (name.startsWith("on") || value.includes("javascript:") || value.includes("url(")) return false;
      if ((name === "href" || name.endsWith(":href")) && value !== "" && !value.startsWith("#") && !value.startsWith("data:image/")) return false;
      return true;
    }),
  );
}
