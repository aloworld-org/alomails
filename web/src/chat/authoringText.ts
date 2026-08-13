export function chatAuthoringText(html: string): string {
  const document = new DOMParser().parseFromString(html, "text/html");
  const equation = document.querySelector<HTMLElement>("[data-alo-latex]");
  if (equation !== null) return `$${equation.dataset.aloLatex ?? ""}$`;
  const code = document.querySelector<HTMLElement>("[data-alo-lang]");
  if (code !== null) return `\`\`\`${code.dataset.aloLang ?? ""}\n${code.textContent ?? ""}\n\`\`\``;
  return document.body.textContent ?? "";
}
