import { invoke } from "@tauri-apps/api/core";
import { MOCK_APPLICATIONS } from "../data/mock-applications";
import type { Application } from "../types/application";
import { isTauri } from "./window";

export interface NamePart {
  text: string;
  match: boolean;
}

function words(name: string): string[] {
  return name.toLowerCase().split(/[\s\-_.]+/).filter(Boolean);
}

function isSubsequence(text: string, keyword: string): boolean {
  let index = 0;
  for (const char of text) {
    if (char === keyword[index]) index += 1;
    if (index === keyword.length) return true;
  }
  return false;
}

export function scoreApplication(app: Application, keyword: string): number {
  const query = keyword.trim().toLowerCase();
  if (!query) return 0;

  const name = app.name.toLowerCase();
  const tokens = words(app.name);

  if (name === query) return 1000;
  if (name.startsWith(query)) return 800;
  if (tokens.some((token) => token.startsWith(query))) return 700;
  if (name.includes(query)) return 600;
  if (tokens.join("").startsWith(query) || tokens.map((token) => token[0]).join("").includes(query)) {
    return 500;
  }
  if (isSubsequence(name, query)) return 300;
  return 0;
}

export function filterApplications(
  apps: Application[],
  keyword: string,
  options: { limit: number; enableUsageRanking: boolean },
): Application[] {
  const query = keyword.trim();

  if (!query) {
    return [...apps]
      .sort((left, right) => {
        const time = (right.lastLaunchTime ?? 0) - (left.lastLaunchTime ?? 0);
        if (time !== 0) return time;
        return right.launchCount - left.launchCount;
      })
      .slice(0, options.limit);
  }

  return apps
    .map((app) => ({ app, score: scoreApplication(app, query) }))
    .filter((item) => item.score > 0)
    .sort((left, right) => {
      if (right.score !== left.score) return right.score - left.score;
      if (options.enableUsageRanking && right.app.launchCount !== left.app.launchCount) {
        return right.app.launchCount - left.app.launchCount;
      }
      return (right.app.lastLaunchTime ?? 0) - (left.app.lastLaunchTime ?? 0);
    })
    .slice(0, options.limit)
    .map((item) => item.app);
}

export function highlightName(name: string, keyword: string): NamePart[] {
  const query = keyword.trim();
  if (!query) return [{ text: name, match: false }];

  const index = name.toLowerCase().indexOf(query.toLowerCase());
  if (index < 0) return [{ text: name, match: false }];

  return [
    { text: name.slice(0, index), match: false },
    { text: name.slice(index, index + query.length), match: true },
    { text: name.slice(index + query.length), match: false },
  ].filter((part) => part.text);
}

export async function searchApplications(keyword: string): Promise<Application[]> {
  if (!isTauri()) {
    return filterApplications(MOCK_APPLICATIONS, keyword, {
      limit: 8,
      enableUsageRanking: true,
    });
  }
  return invoke("search_applications", { keyword });
}
