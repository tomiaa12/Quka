import { convertFileSrc } from "@tauri-apps/api/core";
import { iconSvg } from "../data/app-icons";
import { isTauri } from "./window";

const PATH_LIKE = /[\\/]|\.png$/i;

export function isCachedIconPath(icon?: string): boolean {
  return Boolean(icon && PATH_LIKE.test(icon));
}

export function appIconImageSrc(icon: string): string {
  return isTauri() ? convertFileSrc(icon) : icon;
}

export function appIconSvg(icon?: string): string {
  if (!icon || isCachedIconPath(icon)) return iconSvg("generic");
  return iconSvg(icon);
}
