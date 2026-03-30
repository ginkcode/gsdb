import { writable } from "svelte/store";
import { browser } from "$app/environment";

export type Platform = "windows" | "macos" | "linux" | "unknown";

function detectPlatform(): Platform {
	if (!browser) return "unknown";
	
	// Check if running in Tauri
	if (window.__TAURI__) {
		const platform = navigator.platform.toLowerCase();
		if (platform.includes("win")) return "windows";
		if (platform.includes("mac")) return "macos";
		if (platform.includes("linux")) return "linux";
	}
	
	return "unknown";
}

function createPlatformStore() {
	const { subscribe, set } = writable<Platform>(detectPlatform());

	return {
		subscribe,
		refresh: () => set(detectPlatform())
	};
}

export const platform = createPlatformStore();

// Helper to get border radius class based on platform
export function getBorderRadiusClass(): string {
	const currentPlatform = detectPlatform();
	if (currentPlatform === "windows") {
		return "rounded-md";
	}
	return "rounded-xl";
}