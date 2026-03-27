import { writable } from "svelte/store";
import { browser } from "$app/environment";
import { invoke } from "@tauri-apps/api/core";

export type Theme = "light" | "dark" | "system";

const STORAGE_KEY = "theme-preference";

// Cache for system theme (set by Tauri command)
let cachedSystemTheme: "light" | "dark" = "light";

export async function getSystemTheme(): Promise<"light" | "dark"> {
	if (!browser) return "light";
	try {
		const theme = await invoke<string>("get_system_theme");
		cachedSystemTheme = theme === "dark" ? "dark" : "light";
		return cachedSystemTheme;
	} catch {
		// Fallback to matchMedia if Tauri command fails
		return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
	}
}

function getStoredTheme(): Theme {
	if (!browser) return "system";
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === "light" || stored === "dark" || stored === "system") {
		return stored;
	}
	return "system";
}

function createThemeStore() {
	const { subscribe, set, update } = writable<Theme>(getStoredTheme());

	return {
		subscribe,
		set: (theme: Theme) => {
			if (browser) {
				localStorage.setItem(STORAGE_KEY, theme);
			}
			set(theme);
		},
		toggle: () => {
			update((current) => {
				const newTheme = current === "system" 
					? cachedSystemTheme === "dark" ? "light" : "dark"
					: current === "dark" ? "light" : "dark";
				if (browser) {
					localStorage.setItem(STORAGE_KEY, newTheme);
				}
				return newTheme;
			});
		},
		getResolvedTheme: (theme: Theme): "light" | "dark" => {
			return theme === "system" ? cachedSystemTheme : theme;
		},
		setSystemTheme: (theme: "light" | "dark") => {
			cachedSystemTheme = theme;
		}
	};
}

export const theme = createThemeStore();