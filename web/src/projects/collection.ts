import type { Project } from "./types";

/** Reconciles one authoritative project response into the loaded portfolio. */
export function upsertProject(projects: Project[], project: Project): Project[] {
  return [...projects.filter((existing) => existing.id !== project.id), project];
}
